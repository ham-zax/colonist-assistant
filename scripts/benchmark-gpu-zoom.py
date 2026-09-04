#!/usr/bin/env python3
"""GPU-first strategic model zoom benchmark.

This is deliberately not a replacement for the native whole-game arena. It
reuses the arena's expert JSONL contract and measures how quickly a GPU can
train/evaluate the compact strategic value/policy architecture over many fixed
positions.

Example:
  uv run --with torch --with numpy \
    python scripts/benchmark-gpu-zoom.py training-data/gpu-zoom-expert.jsonl \
    --device cuda --output benchmark-results/gpu-zoom.json
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import time
from pathlib import Path
from typing import Iterable

import numpy as np

try:
    import torch
    from torch import nn
except ModuleNotFoundError as error:  # pragma: no cover - user-facing dependency gate
    raise SystemExit(
        "PyTorch is required. Run with: uv run --with torch --with numpy "
        "python scripts/benchmark-gpu-zoom.py ..."
    ) from error


SCRIPT_DIR = Path(__file__).resolve().parent
TRAINER_PATH = SCRIPT_DIR / "train-strategic-model.py"


def load_trainer_module():
    spec = importlib.util.spec_from_file_location("colonist_strategic_trainer", TRAINER_PATH)
    if spec is None or spec.loader is None:
        raise SystemExit(f"Cannot import {TRAINER_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


trainer = load_trainer_module()


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Train and benchmark the current compact strategic value/policy "
            "architecture on a GPU using fixed arena expert samples."
        )
    )
    parser.add_argument("inputs", nargs="+", type=Path)
    parser.add_argument("--output", type=Path, default=Path("benchmark-results/gpu-zoom.json"))
    parser.add_argument("--checkpoint", type=Path)
    parser.add_argument(
        "--device",
        choices=["auto", "cuda", "cpu"],
        default="cuda",
        help=(
            "Execution device (default: cuda). 'auto' may fall back to CPU and "
            "must be requested explicitly."
        ),
    )
    parser.add_argument("--hidden", type=int, default=32)
    parser.add_argument("--epochs", type=int, default=8)
    parser.add_argument("--policy-epochs", type=int, default=4)
    parser.add_argument("--batch-size", type=int, default=4096)
    parser.add_argument("--policy-batch-groups", type=int, default=512)
    parser.add_argument("--learning-rate", type=float, default=0.004)
    parser.add_argument("--validation-fraction", type=float, default=0.15)
    parser.add_argument("--validation-folds", type=int, default=1)
    parser.add_argument("--validation-fold-index", type=int, default=0)
    parser.add_argument(
        "--baseline-action-features",
        type=int,
        default=0,
        help="Zero action features at and after this index for the matched Task 15 ablation.",
    )
    parser.add_argument("--max-samples", type=int, default=0)
    parser.add_argument("--seed", type=int, default=20260728)
    parser.add_argument("--inference-repeats", type=int, default=100)
    return parser.parse_args()


def choose_device(requested: str) -> torch.device:
    if requested == "cpu":
        return torch.device("cpu")
    if requested == "cuda":
        if not torch.cuda.is_available():
            raise SystemExit("--device cuda requested but torch.cuda.is_available() is false")
        return torch.device("cuda")
    return torch.device("cuda" if torch.cuda.is_available() else "cpu")


def sync(device: torch.device) -> None:
    if device.type == "cuda":
        torch.cuda.synchronize(device)


def teacher_records(paths: Iterable[Path]) -> list[dict]:
    data = trainer.records(paths)
    if sorted({int(record.get("schemaVersion", 1)) for record in data}) != [2]:
        raise SystemExit("GPU zoom requires Task 15 strategic feature schema 2")
    teacher_engines = sorted({str(record.get("engine", "")) for record in data})
    if len(teacher_engines) != 1 or teacher_engines[0] not in {
        "puct-teacher",
        "native-gpu-teacher",
    }:
        raise SystemExit(
            "GPU zoom requires a uniform single-teacher corpus "
            "('puct-teacher' or 'native-gpu-teacher'); "
            f"got {teacher_engines}. Refusing to mix incompatible policy semantics."
        )
    if teacher_engines[0] == "puct-teacher":
        return [record for record in data if trainer.has_usable_teacher_value(record)]
    return [record for record in data if trainer.has_usable_policy_teacher(record)]


def held_out_groups(
    values: Iterable[str], validation_fraction: float, fold_count: int, fold_index: int
) -> set[str]:
    if fold_count <= 1:
        return trainer.validation_groups(values, validation_fraction)
    unique = sorted(set(values))
    if fold_count > len(unique):
        raise SystemExit("--validation-folds cannot exceed independent board/chance groups")
    if not 0 <= fold_index < fold_count:
        raise SystemExit("--validation-fold-index must be within --validation-folds")
    ranked = sorted(
        unique,
        key=lambda value: hashlib.sha256(value.encode("utf-8")).digest(),
    )
    return set(ranked[fold_index::fold_count])


def initialize_linear(layer: nn.Linear, weight_std: float) -> None:
    nn.init.normal_(layer.weight, mean=0.0, std=weight_std)
    nn.init.zeros_(layer.bias)


class ValueNet(nn.Module):
    def __init__(self, width: int, hidden: int) -> None:
        super().__init__()
        self.first = nn.Linear(width, hidden)
        self.second = nn.Linear(hidden, 4)
        initialize_linear(self.first, 0.045)
        initialize_linear(self.second, 0.060)

    def forward(self, values: torch.Tensor) -> torch.Tensor:
        return self.second(torch.relu(self.first(values)))


class PolicyNet(nn.Module):
    def __init__(self, width: int, hidden: int) -> None:
        super().__init__()
        self.first = nn.Linear(width, hidden)
        self.second = nn.Linear(hidden, 1)
        initialize_linear(self.first, 0.035)
        initialize_linear(self.second, 0.050)

    def forward(self, values: torch.Tensor) -> torch.Tensor:
        return self.second(torch.relu(self.first(values))).squeeze(-1)


def mask_value_logits(logits: torch.Tensor, players: torch.Tensor) -> torch.Tensor:
    columns = torch.arange(4, device=logits.device).unsqueeze(0)
    return logits.masked_fill(columns >= players.unsqueeze(1), -1e9)


def segment_log_softmax(
    logits: torch.Tensor, group_ids: torch.Tensor, group_count: int
) -> torch.Tensor:
    maxima = torch.full(
        (group_count,),
        -torch.inf,
        dtype=logits.dtype,
        device=logits.device,
    )
    maxima.scatter_reduce_(0, group_ids, logits, reduce="amax", include_self=True)
    shifted = logits - maxima[group_ids]
    exponentials = shifted.exp()
    totals = torch.zeros(group_count, dtype=logits.dtype, device=logits.device)
    totals.scatter_add_(0, group_ids, exponentials)
    return shifted - totals[group_ids].clamp_min(1e-12).log()


def policy_batch(
    groups: list[tuple[np.ndarray, np.ndarray, str, np.ndarray]],
    indices: np.ndarray,
    device: torch.device,
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor, int]:
    features = []
    targets = []
    ids = []
    for local, source_index in enumerate(indices):
        group_features, group_target, _, _ = groups[int(source_index)]
        features.append(group_features)
        targets.append(group_target)
        ids.append(np.full(len(group_target), local, dtype=np.int64))
    return (
        torch.from_numpy(np.concatenate(features)).to(device, non_blocking=True),
        torch.from_numpy(np.concatenate(targets)).to(device, non_blocking=True),
        torch.from_numpy(np.concatenate(ids)).to(device, non_blocking=True),
        len(indices),
    )


def train_value(
    model: ValueNet,
    x: np.ndarray,
    y: np.ndarray,
    players: np.ndarray,
    train_indices: np.ndarray,
    args: argparse.Namespace,
    device: torch.device,
    rng: np.random.Generator,
) -> tuple[float, int]:
    optimizer = torch.optim.SGD(model.parameters(), lr=args.learning_rate)
    processed = 0
    sync(device)
    started = time.perf_counter()
    for epoch in range(args.epochs):
        learning_rate = args.learning_rate * (0.35 + 0.65 * (1 - epoch / max(1, args.epochs)))
        for parameter_group in optimizer.param_groups:
            parameter_group["lr"] = learning_rate
        shuffled = rng.permutation(train_indices)
        for start in range(0, len(shuffled), args.batch_size):
            indices = shuffled[start : start + args.batch_size]
            batch_x = torch.from_numpy(x[indices]).to(device, non_blocking=True)
            batch_players = torch.from_numpy(players[indices]).to(device, non_blocking=True)
            targets = torch.from_numpy(y[indices]).to(device, non_blocking=True)
            logits = mask_value_logits(model(batch_x), batch_players)
            log_probabilities = nn.functional.log_softmax(logits, dim=1)
            loss = -(targets * log_probabilities).sum(dim=1).mean()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_value_(model.parameters(), 1.5)
            optimizer.step()
            processed += len(indices)
    sync(device)
    return time.perf_counter() - started, processed


def train_policy(
    model: PolicyNet,
    groups: list[tuple[np.ndarray, np.ndarray, str, np.ndarray]],
    train_indices: np.ndarray,
    args: argparse.Namespace,
    device: torch.device,
    rng: np.random.Generator,
) -> tuple[float, int, int]:
    optimizer = torch.optim.SGD(model.parameters(), lr=args.learning_rate)
    processed_groups = 0
    processed_actions = 0
    sync(device)
    started = time.perf_counter()
    for epoch in range(args.policy_epochs):
        learning_rate = args.learning_rate * (
            0.35 + 0.65 * (1 - epoch / max(1, args.policy_epochs))
        )
        for parameter_group in optimizer.param_groups:
            parameter_group["lr"] = learning_rate
        shuffled = rng.permutation(train_indices)
        for start in range(0, len(shuffled), args.policy_batch_groups):
            indices = shuffled[start : start + args.policy_batch_groups]
            features, targets, group_ids, group_count = policy_batch(groups, indices, device)
            logits = model(features)
            log_probabilities = segment_log_softmax(logits, group_ids, group_count)
            loss = -(targets * log_probabilities).sum() / max(1, group_count)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_value_(model.parameters(), 1.5)
            optimizer.step()
            processed_groups += group_count
            processed_actions += len(targets)
    sync(device)
    return time.perf_counter() - started, processed_groups, processed_actions


def value_metrics(
    model: ValueNet,
    x: np.ndarray,
    y: np.ndarray,
    players: np.ndarray,
    indices: np.ndarray,
    device: torch.device,
    temperature: float | None = None,
) -> dict:
    with torch.inference_mode():
        batch_x = torch.from_numpy(x[indices]).to(device)
        batch_players = torch.from_numpy(players[indices]).to(device)
        logits = mask_value_logits(model(batch_x), batch_players)
        targets = torch.from_numpy(y[indices]).to(device)
        if temperature is None:
            candidates = torch.linspace(0.65, 2.5, 75, device=device)
            losses = []
            for candidate_temperature in candidates:
                probabilities = torch.softmax(logits / candidate_temperature, dim=1)
                losses.append(
                    -(targets * probabilities.clamp_min(1e-8).log()).sum(dim=1).mean()
                )
            losses_tensor = torch.stack(losses)
            best_index = int(losses_tensor.argmin().item())
            temperature = float(candidates[best_index].item())
        probabilities = torch.softmax(logits / temperature, dim=1)
        brier = ((probabilities - targets) ** 2).sum(dim=1).mean()
        log_loss = -(targets * probabilities.clamp_min(1e-8).log()).sum(dim=1).mean()
        accuracy = (probabilities.argmax(dim=1) == targets.argmax(dim=1)).float().mean()
        uniform = torch.zeros_like(probabilities)
        for player_count in range(2, 5):
            members = batch_players == player_count
            if members.any():
                uniform[members, :player_count] = 1.0 / player_count
        uniform_brier = ((uniform - targets) ** 2).sum(dim=1).mean()
        uniform_log_loss = -(targets * uniform.clamp_min(1e-8).log()).sum(dim=1).mean()
    return {
        "valueBrier": float(brier.item()),
        "valueLogLoss": float(log_loss.item()),
        "valueTop1Accuracy": float(accuracy.item()),
        "uniformValueBrier": float(uniform_brier.item()),
        "uniformValueLogLoss": float(uniform_log_loss.item()),
        "temperature": temperature,
    }


def policy_metrics(
    model: PolicyNet,
    groups: list[tuple[np.ndarray, np.ndarray, str, np.ndarray]],
    indices: np.ndarray,
    batch_groups: int,
    device: torch.device,
    teacher_engine: str,
) -> dict:
    weighted_loss = 0.0
    uniform_loss = 0.0
    seen = 0
    action_regrets = []
    expected_regrets = []
    top1_matches = []
    with torch.inference_mode():
        for start in range(0, len(indices), batch_groups):
            batch = indices[start : start + batch_groups]
            features, targets, group_ids, group_count = policy_batch(groups, batch, device)
            logits = model(features)
            log_probabilities = segment_log_softmax(logits, group_ids, group_count)
            per_action_loss = -(targets * log_probabilities)
            group_losses = torch.zeros(group_count, device=device)
            group_losses.scatter_add_(0, group_ids, per_action_loss)
            weighted_loss += float(group_losses.sum().item())
            uniform_loss += sum(math.log(max(1, len(groups[int(index)][1]))) for index in batch)
            seen += group_count
        for index in indices:
            features, target, _, teacher_values = groups[int(index)]
            logits = model(torch.from_numpy(features).to(device))
            probabilities = torch.softmax(logits, dim=0)
            selected = int(logits.argmax().item())
            if teacher_engine == "native-gpu-teacher":
                # Native GPU policy labels are one-hot on the final authoritative
                # decision. Its per-action value field is mean candidate VP, while
                # final root selection uses terminal outcome, victory margin,
                # tie-breaks, and possible exact/safety replacement. VP therefore
                # cannot define a compatible scalar regret or value-head target.
                top1_matches.append(selected == int(target.argmax()))
                continue
            teacher = torch.from_numpy(teacher_values).to(device)
            teacher_best = teacher.max()
            action_regrets.append(float((teacher_best - teacher[selected]).clamp_min(0).item()))
            expected_regrets.append(
                float((teacher_best - torch.dot(probabilities, teacher)).clamp_min(0).item())
            )
            top1_matches.append(selected == int(teacher.argmax().item()))
    return {
        "policyCrossEntropy": weighted_loss / max(1, seen),
        "uniformPolicyCrossEntropy": uniform_loss / max(1, seen),
        "policyMeanActionRegret": float(np.mean(action_regrets)) if action_regrets else None,
        "policyMeanExpectedActionRegret": (
            float(np.mean(expected_regrets)) if expected_regrets else None
        ),
        "policyTeacherTop1Agreement": float(np.mean(top1_matches)) if top1_matches else None,
    }


def inference_throughput(
    value_model: ValueNet | None,
    policy_model: PolicyNet,
    x: np.ndarray,
    groups: list[tuple[np.ndarray, np.ndarray, str, np.ndarray]],
    args: argparse.Namespace,
    device: torch.device,
) -> dict:
    value_count = min(max(len(x), args.batch_size), 65_536)
    value_tensor = None
    if value_model is not None:
        value_source = np.resize(x, (value_count, x.shape[1])).astype(np.float32, copy=False)
        value_tensor = torch.from_numpy(value_source).to(device)
    policy_source = np.concatenate([group[0] for group in groups[: min(len(groups), 2048)]])
    if len(policy_source) < args.batch_size:
        policy_source = np.resize(
            policy_source,
            (args.batch_size, policy_source.shape[1]),
        ).astype(np.float32, copy=False)
    policy_tensor = torch.from_numpy(policy_source).to(device)

    warmup = min(10, args.inference_repeats)
    value_seconds = None
    with torch.inference_mode():
        for _ in range(warmup):
            if value_model is not None and value_tensor is not None:
                value_model(value_tensor)
            policy_model(policy_tensor)
        sync(device)
        if value_model is not None and value_tensor is not None:
            started = time.perf_counter()
            for _ in range(args.inference_repeats):
                value_model(value_tensor)
            sync(device)
            value_seconds = time.perf_counter() - started
        started = time.perf_counter()
        for _ in range(args.inference_repeats):
            policy_model(policy_tensor)
        sync(device)
        policy_seconds = time.perf_counter() - started
    return {
        "valuePositionsPerSecond": (
            value_count * args.inference_repeats / max(value_seconds, 1e-9)
            if value_seconds is not None
            else None
        ),
        "policyActionsPerSecond": len(policy_tensor) * args.inference_repeats / max(policy_seconds, 1e-9),
        "valueBatch": value_count if value_model is not None else None,
        "policyBatch": len(policy_tensor),
    }


def main() -> None:
    args = arguments()
    if args.hidden < 1 or args.batch_size < 1 or args.policy_batch_groups < 1:
        raise SystemExit("hidden and batch sizes must be positive")
    baseline_requested = args.baseline_action_features > 0
    if baseline_requested and not trainer.is_task15_baseline(0, args.baseline_action_features):
        raise SystemExit(
            "Task 15 GPU feature comparison requires the exact old representation: "
            "--baseline-action-features 48. Omit the option for a candidate-only "
            "screen."
        )
    device = choose_device(args.device)
    np.random.seed(args.seed)
    torch.manual_seed(args.seed)
    if device.type == "cuda":
        torch.cuda.manual_seed_all(args.seed)
        torch.cuda.reset_peak_memory_stats(device)

    data = teacher_records(args.inputs)
    if args.max_samples > 0:
        data = data[: args.max_samples]
    if len(data) < 50:
        raise SystemExit("Need at least 50 expert teacher samples")
    teacher_engine = str(data[0]["engine"])
    value_teacher_available = teacher_engine == "puct-teacher"
    if args.checkpoint and not value_teacher_available:
        raise SystemExit(
            "--checkpoint requires puct-teacher value semantics; native-gpu-teacher "
            "records are policy-only screening evidence"
        )

    x = np.asarray([record["stateFeatures"] for record in data], dtype=np.float32)
    y = (
        np.asarray(
            [trainer.relative_teacher_value(record) for record in data], dtype=np.float32
        )
        if value_teacher_available
        else None
    )
    players = (
        np.asarray([record["players"] for record in data], dtype=np.int64)
        if value_teacher_available
        else None
    )
    state_groups = [trainer.synthetic_game_group(record) for record in data]
    held_out_group_keys = held_out_groups(
        state_groups,
        args.validation_fraction,
        args.validation_folds,
        args.validation_fold_index,
    )
    held_out = np.asarray([group in held_out_group_keys for group in state_groups], dtype=bool)
    train_indices = np.flatnonzero(~held_out)
    validation_indices = np.flatnonzero(held_out)

    groups = trainer.policy_groups(data)
    if not groups:
        raise SystemExit("Expert data contains no searched action groups")
    group_validation = np.asarray(
        [group[2] in held_out_group_keys for group in groups], dtype=bool
    )
    policy_train = np.flatnonzero(~group_validation)
    policy_validation = np.flatnonzero(group_validation)
    if not len(policy_train) or not len(policy_validation):
        raise SystemExit("Grouped policy split must contain training and validation games")

    action_widths = sorted({group[0].shape[1] - x.shape[1] for group in groups})
    if len(action_widths) != 1 or not trainer.is_task15_action_width(action_widths[0]):
        raise SystemExit(
            "GPU zoom requires exactly 52 Task 15 action features, "
            f"got {action_widths}"
        )
    action_width = action_widths[0]
    if args.baseline_action_features < 0 or args.baseline_action_features > action_width:
        raise SystemExit("--baseline-action-features must be within the action feature width")
    value_model = ValueNet(x.shape[1], args.hidden).to(device) if value_teacher_available else None
    policy_model = PolicyNet(groups[0][0].shape[1], args.hidden).to(device)
    rng = np.random.default_rng(args.seed)

    value_seconds = None
    value_examples = 0
    if value_model is not None and y is not None and players is not None:
        value_seconds, value_examples = train_value(
            value_model,
            x,
            y,
            players,
            train_indices,
            args,
            device,
            rng,
        )
    policy_seconds, policy_groups_seen, policy_actions_seen = train_policy(
        policy_model,
        groups,
        policy_train,
        args,
        device,
        rng,
    )
    training_metrics = {}
    if value_model is not None and y is not None and players is not None:
        training_metrics.update(
            value_metrics(value_model, x, y, players, train_indices, device)
        )
    training_metrics.update(
        policy_metrics(
            policy_model,
            groups,
            policy_train,
            args.policy_batch_groups,
            device,
            teacher_engine,
        )
    )
    metrics = {}
    if value_model is not None and y is not None and players is not None:
        metrics.update(
            value_metrics(
                value_model,
                x,
                y,
                players,
                validation_indices,
                device,
                temperature=training_metrics["temperature"],
            )
        )
    metrics.update(
        policy_metrics(
            policy_model,
            groups,
            policy_validation,
            args.policy_batch_groups,
            device,
            teacher_engine,
        )
    )
    metrics["throughput"] = {
        "valueTrainingExamplesPerSecond": (
            value_examples / max(value_seconds, 1e-9) if value_seconds is not None else None
        ),
        "policyTrainingGroupsPerSecond": policy_groups_seen / max(policy_seconds, 1e-9),
        "policyTrainingActionsPerSecond": policy_actions_seen / max(policy_seconds, 1e-9),
        "valueTrainingSeconds": value_seconds,
        "policyTrainingSeconds": policy_seconds,
        **inference_throughput(value_model, policy_model, x, groups, args, device),
    }

    baseline_metrics = None
    baseline_training_metrics = None
    feature_comparison = None
    if args.baseline_action_features > 0:
        baseline_groups = trainer.policy_groups(
            data, zero_action_tail_from=args.baseline_action_features
        )
        torch.manual_seed(args.seed)
        if device.type == "cuda":
            torch.cuda.manual_seed_all(args.seed)
        baseline_value_model = (
            ValueNet(x.shape[1], args.hidden).to(device) if value_teacher_available else None
        )
        baseline_policy_model = PolicyNet(
            baseline_groups[0][0].shape[1], args.hidden
        ).to(device)
        baseline_rng = np.random.default_rng(args.seed)
        if baseline_value_model is not None and y is not None and players is not None:
            train_value(
                baseline_value_model,
                x,
                y,
                players,
                train_indices,
                args,
                device,
                baseline_rng,
            )
        train_policy(
            baseline_policy_model,
            baseline_groups,
            policy_train,
            args,
            device,
            baseline_rng,
        )
        baseline_training_metrics = {}
        if baseline_value_model is not None and y is not None and players is not None:
            baseline_training_metrics.update(
                value_metrics(
                    baseline_value_model, x, y, players, train_indices, device
                )
            )
        baseline_training_metrics.update(
            policy_metrics(
                baseline_policy_model,
                baseline_groups,
                policy_train,
                args.policy_batch_groups,
                device,
                teacher_engine,
            )
        )
        baseline_metrics = {}
        if baseline_value_model is not None and y is not None and players is not None:
            baseline_metrics.update(
                value_metrics(
                    baseline_value_model,
                    x,
                    y,
                    players,
                    validation_indices,
                    device,
                    temperature=baseline_training_metrics["temperature"],
                )
            )
        baseline_metrics.update(
            policy_metrics(
                baseline_policy_model,
                baseline_groups,
                policy_validation,
                args.policy_batch_groups,
                device,
                teacher_engine,
            )
        )
        tolerance = 1e-6
        policy_cross_entropy_improvement = (
            baseline_metrics["policyCrossEntropy"] - metrics["policyCrossEntropy"]
        )
        top1_improvement = (
            metrics["policyTeacherTop1Agreement"]
            - baseline_metrics["policyTeacherTop1Agreement"]
        )
        feature_comparison = {
            "policyCrossEntropyImprovement": policy_cross_entropy_improvement,
            "policyTeacherTop1AgreementImprovement": top1_improvement,
            "policyCrossEntropyPassed": policy_cross_entropy_improvement >= -tolerance,
            "teacherTop1AgreementPassed": top1_improvement >= -tolerance,
            "valueLogLossImprovement": None,
            "valueBrierImprovement": None,
            "policyActionRegretImprovement": None,
            "policyExpectedRegretImprovement": None,
            "valueLogLossPassed": None,
            "valueBrierPassed": None,
            "actionRegretPassed": None,
            "expectedRegretPassed": None,
        }
        if value_teacher_available:
            action_regret_improvement = (
                baseline_metrics["policyMeanActionRegret"]
                - metrics["policyMeanActionRegret"]
            )
            expected_regret_improvement = (
                baseline_metrics["policyMeanExpectedActionRegret"]
                - metrics["policyMeanExpectedActionRegret"]
            )
            value_log_loss_improvement = (
                baseline_metrics["valueLogLoss"] - metrics["valueLogLoss"]
            )
            value_brier_improvement = baseline_metrics["valueBrier"] - metrics["valueBrier"]
            feature_comparison.update(
                {
                    "valueLogLossImprovement": value_log_loss_improvement,
                    "valueBrierImprovement": value_brier_improvement,
                    "policyActionRegretImprovement": action_regret_improvement,
                    "policyExpectedRegretImprovement": expected_regret_improvement,
                    "valueLogLossPassed": value_log_loss_improvement >= -tolerance,
                    "valueBrierPassed": value_brier_improvement >= -tolerance,
                    "actionRegretPassed": action_regret_improvement >= -tolerance,
                    "expectedRegretPassed": expected_regret_improvement >= -tolerance,
                }
            )
            feature_comparison["accepted"] = all(
                feature_comparison[key]
                for key in (
                    "policyCrossEntropyPassed",
                    "valueLogLossPassed",
                    "valueBrierPassed",
                    "actionRegretPassed",
                    "expectedRegretPassed",
                )
            )
        else:
            # Native-GPU screening has a final-choice policy teacher only. The
            # closest valid decision-quality metrics are held-out policy
            # cross-entropy and final chosen-action agreement.
            feature_comparison["accepted"] = bool(
                feature_comparison["policyCrossEntropyPassed"]
                and feature_comparison["teacherTop1AgreementPassed"]
            )

    training_group_keys = sorted(set(state_groups) - held_out_group_keys)
    validation_group_keys = sorted(held_out_group_keys)
    result = {
        "schemaVersion": 1,
        "kind": "colonist-gpu-zoom-benchmark",
        "device": str(device),
        "torchVersion": torch.__version__,
        "cudaVersion": torch.version.cuda,
        "gpu": torch.cuda.get_device_name(device) if device.type == "cuda" else None,
        "samples": len(data),
        "trainingSamples": len(train_indices),
        "validationSamples": len(validation_indices),
        "policyGroups": len(groups),
        "policyActions": int(sum(len(group[1]) for group in groups)),
        "stateFeatures": int(x.shape[1]),
        "actionFeatures": int(action_width),
        "baselineActionFeatures": (
            args.baseline_action_features if args.baseline_action_features > 0 else None
        ),
        "baselineEvaluated": bool(
            trainer.is_task15_baseline(0, args.baseline_action_features)
            and baseline_metrics is not None
        ),
        "featureAblationPassed": bool(
            trainer.is_task15_baseline(0, args.baseline_action_features)
            and feature_comparison is not None
            and feature_comparison.get("accepted") is True
        ),
        "teacherEngines": [teacher_engine],
        "valueTeacherAvailable": value_teacher_available,
        "valueTargetSemantics": (
            "normalized-strategic-relative" if value_teacher_available else None
        ),
        "policyTargetSemantics": (
            "puct-visit-share"
            if teacher_engine == "puct-teacher"
            else "native-final-choice-one-hot"
        ),
        "trainingStateGroups": len(training_group_keys),
        "validationStateGroups": len(validation_group_keys),
        "policyTrainingGroups": len(policy_train),
        "policyValidationGroups": len(policy_validation),
        "validationFolds": args.validation_folds,
        "validationFoldIndex": args.validation_fold_index,
        "trainingGroupKeys": training_group_keys,
        "validationGroupKeys": validation_group_keys,
        "hidden": args.hidden,
        "epochs": args.epochs if value_teacher_available else 0,
        "policyEpochs": args.policy_epochs,
        "batchSize": args.batch_size,
        "policyBatchGroups": args.policy_batch_groups,
        "seed": args.seed,
        "trainingMetrics": training_metrics,
        "metrics": metrics,
        "baselineTrainingMetrics": baseline_training_metrics,
        "baselineHeldOutMetrics": baseline_metrics,
        "featureComparison": feature_comparison,
        "peakGpuMemoryMiB": (
            torch.cuda.max_memory_allocated(device) / (1024 * 1024)
            if device.type == "cuda"
            else None
        ),
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    if args.checkpoint:
        assert value_model is not None
        args.checkpoint.parent.mkdir(parents=True, exist_ok=True)
        torch.save(
            {
                "value": value_model.state_dict(),
                "policy": policy_model.state_dict(),
                "metadata": result,
            },
            args.checkpoint,
        )
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
