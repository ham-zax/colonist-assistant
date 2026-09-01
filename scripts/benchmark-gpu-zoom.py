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
    parser.add_argument("--device", choices=["auto", "cuda", "cpu"], default="auto")
    parser.add_argument("--hidden", type=int, default=32)
    parser.add_argument("--epochs", type=int, default=8)
    parser.add_argument("--policy-epochs", type=int, default=4)
    parser.add_argument("--batch-size", type=int, default=4096)
    parser.add_argument("--policy-batch-groups", type=int, default=512)
    parser.add_argument("--learning-rate", type=float, default=0.004)
    parser.add_argument("--validation-fraction", type=float, default=0.15)
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


def terminal_records(paths: Iterable[Path]) -> list[dict]:
    return [
        record
        for record in trainer.records(paths)
        if bool(record.get("terminal", sum(record.get("outcome", [])) > 0))
        and abs(sum(record.get("outcome", [])) - 1.0) < 1e-5
    ]


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
    groups: list[tuple[np.ndarray, np.ndarray, str]], indices: np.ndarray, device: torch.device
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor, int]:
    features = []
    targets = []
    ids = []
    for local, source_index in enumerate(indices):
        group_features, group_target, _ = groups[int(source_index)]
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
            winners = torch.from_numpy(y[indices].argmax(axis=1)).to(device, non_blocking=True)
            logits = mask_value_logits(model(batch_x), batch_players)
            loss = nn.functional.cross_entropy(logits, winners)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_value_(model.parameters(), 1.5)
            optimizer.step()
            processed += len(indices)
    sync(device)
    return time.perf_counter() - started, processed


def train_policy(
    model: PolicyNet,
    groups: list[tuple[np.ndarray, np.ndarray, str]],
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
) -> dict:
    with torch.inference_mode():
        batch_x = torch.from_numpy(x[indices]).to(device)
        batch_players = torch.from_numpy(players[indices]).to(device)
        logits = mask_value_logits(model(batch_x), batch_players)
        targets = torch.from_numpy(y[indices]).to(device)
        candidates = torch.linspace(0.65, 2.5, 75, device=device)
        losses = []
        for temperature in candidates:
            probabilities = torch.softmax(logits / temperature, dim=1)
            losses.append(-(targets * probabilities.clamp_min(1e-8).log()).sum(dim=1).mean())
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
    groups: list[tuple[np.ndarray, np.ndarray, str]],
    indices: np.ndarray,
    batch_groups: int,
    device: torch.device,
) -> dict:
    weighted_loss = 0.0
    uniform_loss = 0.0
    seen = 0
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
    return {
        "policyCrossEntropy": weighted_loss / max(1, seen),
        "uniformPolicyCrossEntropy": uniform_loss / max(1, seen),
    }


def inference_throughput(
    value_model: ValueNet,
    policy_model: PolicyNet,
    x: np.ndarray,
    groups: list[tuple[np.ndarray, np.ndarray, str]],
    args: argparse.Namespace,
    device: torch.device,
) -> dict:
    value_count = min(max(len(x), args.batch_size), 65_536)
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
    with torch.inference_mode():
        for _ in range(warmup):
            value_model(value_tensor)
            policy_model(policy_tensor)
        sync(device)
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
        "valuePositionsPerSecond": value_count * args.inference_repeats / max(value_seconds, 1e-9),
        "policyActionsPerSecond": len(policy_tensor) * args.inference_repeats / max(policy_seconds, 1e-9),
        "valueBatch": value_count,
        "policyBatch": len(policy_tensor),
    }


def main() -> None:
    args = arguments()
    if args.hidden < 1 or args.batch_size < 1 or args.policy_batch_groups < 1:
        raise SystemExit("hidden and batch sizes must be positive")
    device = choose_device(args.device)
    np.random.seed(args.seed)
    torch.manual_seed(args.seed)
    if device.type == "cuda":
        torch.cuda.manual_seed_all(args.seed)
        torch.cuda.reset_peak_memory_stats(device)

    data = terminal_records(args.inputs)
    if args.max_samples > 0:
        data = data[: args.max_samples]
    if len(data) < 50:
        raise SystemExit("Need at least 50 terminal expert samples")

    x = np.asarray([record["stateFeatures"] for record in data], dtype=np.float32)
    y = np.asarray([trainer.relative_outcome(record) for record in data], dtype=np.float32)
    players = np.asarray([record["players"] for record in data], dtype=np.int64)
    state_groups = [
        f'{record.get("boardSeed", "unknown")}:{record.get("chanceSeed", "unknown")}'
        for record in data
    ]
    held_out_groups = trainer.validation_groups(state_groups, args.validation_fraction)
    held_out = np.asarray([group in held_out_groups for group in state_groups], dtype=bool)
    train_indices = np.flatnonzero(~held_out)
    validation_indices = np.flatnonzero(held_out)

    groups = trainer.policy_groups(data)
    if not groups:
        raise SystemExit("Expert data contains no searched action groups")
    group_validation = np.asarray([group[2] in held_out_groups for group in groups], dtype=bool)
    policy_train = np.flatnonzero(~group_validation)
    policy_validation = np.flatnonzero(group_validation)
    if not len(policy_train) or not len(policy_validation):
        raise SystemExit("Grouped policy split must contain training and validation games")

    value_model = ValueNet(x.shape[1], args.hidden).to(device)
    policy_model = PolicyNet(groups[0][0].shape[1], args.hidden).to(device)
    rng = np.random.default_rng(args.seed)

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
    metrics = value_metrics(value_model, x, y, players, validation_indices, device)
    metrics.update(
        policy_metrics(
            policy_model,
            groups,
            policy_validation,
            args.policy_batch_groups,
            device,
        )
    )
    metrics["throughput"] = {
        "valueTrainingExamplesPerSecond": value_examples / max(value_seconds, 1e-9),
        "policyTrainingGroupsPerSecond": policy_groups_seen / max(policy_seconds, 1e-9),
        "policyTrainingActionsPerSecond": policy_actions_seen / max(policy_seconds, 1e-9),
        "valueTrainingSeconds": value_seconds,
        "policyTrainingSeconds": policy_seconds,
        **inference_throughput(value_model, policy_model, x, groups, args, device),
    }
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
        "actionFeatures": int(groups[0][0].shape[1] - x.shape[1]),
        "hidden": args.hidden,
        "epochs": args.epochs,
        "policyEpochs": args.policy_epochs,
        "batchSize": args.batch_size,
        "policyBatchGroups": args.policy_batch_groups,
        "seed": args.seed,
        "metrics": metrics,
        "peakGpuMemoryMiB": (
            torch.cuda.max_memory_allocated(device) / (1024 * 1024)
            if device.type == "cuda"
            else None
        ),
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    if args.checkpoint:
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
