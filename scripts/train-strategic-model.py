#!/usr/bin/env python3
"""Train the compact graph-readout value and policy models.

Input is JSONL emitted by `colonist-arena --expert-output`. Task 15 schema-2
records used for checkpoint training carry high-budget PUCT policy visits and
normalized strategic-value vectors. Native-GPU final-decision records are
policy-only screening evidence: their rollout VP diagnostics are not compatible
value-head targets and are rejected by this checkpoint trainer. This script performs
deterministic Expert Iteration training and writes Rust constants consumed by the
WASM engine.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Iterable

import numpy as np


def is_task15_baseline(
    baseline_state_features: int, baseline_action_features: int
) -> bool:
    """Return whether an ablation is the exact old Task 15 representation."""
    return baseline_state_features == 0 and baseline_action_features == 48


def is_task15_action_width(action_features: int) -> bool:
    return action_features == 52


def task15_feature_ablation_passed(
    baseline_evaluated: bool, checks: Iterable[bool]
) -> bool:
    return baseline_evaluated and all(checks)


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("inputs", nargs="+", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("engine/crates/catan-search/src/model_weights.rs"),
    )
    parser.add_argument(
        "--metrics",
        type=Path,
        default=Path("benchmark-results/strategic-model-metrics.json"),
    )
    parser.add_argument("--epochs", type=int, default=18)
    parser.add_argument("--policy-epochs", type=int, default=10)
    parser.add_argument("--hidden", type=int, default=32)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--learning-rate", type=float, default=0.004)
    parser.add_argument("--validation-fraction", type=float, default=0.15)
    parser.add_argument("--minimum-validation-groups", type=int, default=20)
    parser.add_argument(
        "--minimum-value-log-loss-gain", type=float, default=0.002
    )
    parser.add_argument(
        "--minimum-value-brier-gain", type=float, default=0.001
    )
    parser.add_argument(
        "--minimum-policy-cross-entropy-gain", type=float, default=0.001
    )
    parser.add_argument("--max-samples", type=int, default=0)
    parser.add_argument("--seed", type=int, default=20260728)
    parser.add_argument(
        "--baseline-state-features",
        type=int,
        default=0,
        help=(
            "For feature-ablation evidence, train a matched baseline with state "
            "features at and after this index zeroed while preserving identical width."
        ),
    )
    parser.add_argument(
        "--baseline-action-features",
        type=int,
        default=0,
        help=(
            "For feature-ablation evidence, zero action features at and after this "
            "index in the matched baseline while preserving identical width."
        ),
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Write an unpromoted checkpoint even when held-out gates fail.",
    )
    return parser.parse_args()


def records(paths: Iterable[Path]) -> list[dict]:
    output: list[dict] = []
    for path in paths:
        with path.open("r", encoding="utf-8") as source:
            for line in source:
                if line.strip():
                    output.append(json.loads(line))
    return output


def has_usable_teacher_value(record: dict) -> bool:
    players = int(record.get("players", 0))
    canonical = record.get("rootSearchValue", [])
    return (
        1 <= players <= 4
        and len(canonical) == 4
        and all(math.isfinite(float(value)) for value in canonical)
        and sum(float(value) for value in canonical[:players]) > 0.0
    )


def has_usable_policy_teacher(record: dict) -> bool:
    players = int(record.get("players", 0))
    actor = int(record.get("actor", -1))
    actions = record.get("actions", [])
    if not (1 <= players <= 4 and 0 <= actor < players and actions):
        return False
    policies = [float(action.get("policy", float("nan"))) for action in actions]
    return policies.count(1.0) == 1 and all(value in (0.0, 1.0) for value in policies)


def relative_teacher_value(record: dict) -> np.ndarray:
    players = int(record["players"])
    actor = int(record["actor"])
    canonical = np.asarray(record["rootSearchValue"], dtype=np.float32)
    if canonical.shape != (4,) or not np.isfinite(canonical).all():
        raise SystemExit("Expert rootSearchValue must be a finite four-player vector")
    target = np.zeros(4, dtype=np.float32)
    for offset in range(players):
        target[offset] = canonical[(actor + offset) % players]
    total = float(target[:players].sum())
    if total <= 0.0:
        raise SystemExit("Expert rootSearchValue must contain positive teacher mass")
    target[:players] /= total
    return target


def softmax(logits: np.ndarray, players: np.ndarray) -> np.ndarray:
    mask = np.arange(4)[None, :] >= players[:, None]
    safe = logits.copy()
    safe[mask] = -1e9
    safe -= safe.max(axis=1, keepdims=True)
    probabilities = np.exp(safe)
    probabilities[mask] = 0
    probabilities /= probabilities.sum(axis=1, keepdims=True).clip(1e-8)
    return probabilities


def train_value(
    x: np.ndarray,
    y: np.ndarray,
    players: np.ndarray,
    train_indices: np.ndarray,
    args: argparse.Namespace,
    rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    hidden = args.hidden
    w1 = rng.normal(0, 0.045, (hidden, x.shape[1])).astype(np.float32)
    b1 = np.zeros(hidden, dtype=np.float32)
    w2 = rng.normal(0, 0.06, (4, hidden)).astype(np.float32)
    b2 = np.zeros(4, dtype=np.float32)
    for epoch in range(args.epochs):
        shuffled = rng.permutation(train_indices)
        learning_rate = args.learning_rate * (0.35 + 0.65 * (1 - epoch / max(1, args.epochs)))
        for start in range(0, len(shuffled), args.batch_size):
            indices = shuffled[start : start + args.batch_size]
            batch_x = x[indices]
            hidden_pre = batch_x @ w1.T + b1
            hidden_value = np.maximum(hidden_pre, 0)
            logits = hidden_value @ w2.T + b2
            probabilities = softmax(logits, players[indices])
            delta = (probabilities - y[indices]) / max(1, len(indices))
            gradient_w2 = delta.T @ hidden_value
            gradient_b2 = delta.sum(axis=0)
            hidden_delta = (delta @ w2) * (hidden_pre > 0)
            gradient_w1 = hidden_delta.T @ batch_x
            gradient_b1 = hidden_delta.sum(axis=0)
            for gradient in (gradient_w1, gradient_b1, gradient_w2, gradient_b2):
                np.clip(gradient, -1.5, 1.5, out=gradient)
            w1 -= learning_rate * gradient_w1
            b1 -= learning_rate * gradient_b1
            w2 -= learning_rate * gradient_w2
            b2 -= learning_rate * gradient_b2
    return w1, b1, w2, b2


def policy_groups(
    data: list[dict],
    zero_state_tail_from: int = 0,
    zero_action_tail_from: int = 0,
) -> list[tuple[np.ndarray, np.ndarray, str, np.ndarray]]:
    groups: list[tuple[np.ndarray, np.ndarray, str, np.ndarray]] = []
    for record in data:
        # Mandatory protocol families and trade responses have exact/dedicated
        # solvers. Training the general strategic prior on them both wastes
        # capacity and can imprint negotiation-loop artifacts on normal play.
        phase_family = str(record.get("phase", "")).split(" ", 1)[0]
        if phase_family in {
            "Discard",
            "MoveRobber",
            "ResolveSteal",
            "RollChance",
            "DevelopmentChance",
            "TradeResponses",
        }:
            continue
        actions = record.get("actions", [])
        if not actions:
            continue
        state = np.asarray(record["stateFeatures"], dtype=np.float32).copy()
        if zero_state_tail_from > 0:
            state[zero_state_tail_from:] = 0.0
        action_rows = []
        for action in actions:
            action_features = np.asarray(action["features"], dtype=np.float32).copy()
            if zero_action_tail_from > 0:
                action_features[zero_action_tail_from:] = 0.0
            action_rows.append(np.concatenate((state, action_features)))
        features = np.asarray(action_rows, dtype=np.float32)
        if not np.isfinite(features).all():
            raise SystemExit("Expert policy features contain NaN or infinite values")
        target = np.asarray([action["policy"] for action in actions], dtype=np.float32)
        target /= target.sum().clip(1e-8)
        actor = int(record["actor"])
        teacher_values = np.asarray(
            [action["searchValue"][actor] for action in actions], dtype=np.float32
        )
        groups.append(
            (
                features,
                target,
                f'{record.get("boardSeed", "unknown")}:{record.get("chanceSeed", "unknown")}',
                teacher_values,
            )
        )
    return groups


def validation_groups(
    values: Iterable[str], validation_fraction: float
) -> set[str]:
    unique = sorted(set(values))
    if len(unique) < 2:
        raise SystemExit(
            "Need at least two independent board/chance seed groups for held-out training"
        )
    ranked = sorted(
        unique,
        key=lambda value: hashlib.sha256(value.encode("utf-8")).digest(),
    )
    count = min(
        len(ranked) - 1,
        max(1, round(len(ranked) * validation_fraction)),
    )
    return set(ranked[:count])


def train_policy(
    groups: list[tuple[np.ndarray, np.ndarray, str, np.ndarray]],
    train_indices: np.ndarray,
    args: argparse.Namespace,
    rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, float]:
    input_width = groups[0][0].shape[1]
    hidden = args.hidden
    w1 = rng.normal(0, 0.035, (hidden, input_width)).astype(np.float32)
    b1 = np.zeros(hidden, dtype=np.float32)
    w2 = rng.normal(0, 0.05, hidden).astype(np.float32)
    b2 = np.float32(0)
    for epoch in range(args.policy_epochs):
        learning_rate = args.learning_rate * (
            0.35 + 0.65 * (1 - epoch / max(1, args.policy_epochs))
        )
        for index in rng.permutation(train_indices):
            features, target, _, _ = groups[int(index)]
            hidden_pre = features @ w1.T + b1
            hidden_value = np.maximum(hidden_pre, 0)
            logits = hidden_value @ w2 + b2
            logits -= logits.max()
            predicted = np.exp(logits)
            predicted /= predicted.sum().clip(1e-8)
            delta = predicted - target
            gradient_w2 = hidden_value.T @ delta
            gradient_b2 = delta.sum()
            hidden_delta = np.outer(delta, w2) * (hidden_pre > 0)
            gradient_w1 = hidden_delta.T @ features
            gradient_b1 = hidden_delta.sum(axis=0)
            for gradient in (gradient_w1, gradient_b1, gradient_w2):
                np.clip(gradient, -1.5, 1.5, out=gradient)
            w1 -= learning_rate * gradient_w1
            b1 -= learning_rate * gradient_b1
            w2 -= learning_rate * gradient_w2
            b2 -= learning_rate * gradient_b2
    return w1, b1, w2, float(b2)


def validation_metrics(
    x: np.ndarray,
    y: np.ndarray,
    players: np.ndarray,
    indices: np.ndarray,
    value_weights: tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray],
    groups: list[tuple[np.ndarray, np.ndarray, str, np.ndarray]],
    policy_indices: np.ndarray,
    policy_weights: tuple[np.ndarray, np.ndarray, np.ndarray, float],
    data: list[dict],
    temperature: float | None = None,
) -> dict:
    w1, b1, w2, b2 = value_weights
    hidden = np.maximum(x[indices] @ w1.T + b1, 0)
    logits = hidden @ w2.T + b2
    # When no temperature is supplied, calibrate on this caller-provided split.
    # Task 15 selects temperature on training groups, then freezes it for the
    # grouped held-out screen so held-out metrics remain untouched by tuning.
    if temperature is None:
        candidates = np.linspace(0.65, 2.5, 75, dtype=np.float32)
        losses = []
        for candidate_temperature in candidates:
            probability = softmax(logits / candidate_temperature, players[indices])
            losses.append(
                -np.sum(y[indices] * np.log(probability.clip(1e-8)), axis=1).mean()
            )
        temperature = float(candidates[int(np.argmin(losses))])
    probability = softmax(logits / temperature, players[indices])
    brier = float(np.sum((probability - y[indices]) ** 2, axis=1).mean())
    log_loss = float(
        -np.sum(y[indices] * np.log(probability.clip(1e-8)), axis=1).mean()
    )
    accuracy = float((probability.argmax(axis=1) == y[indices].argmax(axis=1)).mean())
    uniform = np.zeros_like(probability)
    for row, player_count in enumerate(players[indices]):
        uniform[row, : int(player_count)] = 1.0 / float(player_count)
    uniform_brier = float(np.sum((uniform - y[indices]) ** 2, axis=1).mean())
    uniform_log_loss = float(
        -np.sum(y[indices] * np.log(uniform.clip(1e-8)), axis=1).mean()
    )

    pw1, pb1, pw2, pb2 = policy_weights
    policy_losses = []
    uniform_policy_losses = []
    policy_action_regrets = []
    policy_expected_regrets = []
    policy_top1_matches = []
    for index in policy_indices:
        features, target, _, teacher_values = groups[int(index)]
        policy_logits = np.maximum(features @ pw1.T + pb1, 0) @ pw2 + pb2
        policy_logits -= policy_logits.max()
        predicted = np.exp(policy_logits)
        predicted /= predicted.sum().clip(1e-8)
        policy_losses.append(float(-(target * np.log(predicted.clip(1e-8))).sum()))
        uniform_policy_losses.append(float(np.log(max(1, len(target)))))
        teacher_best = float(teacher_values.max())
        selected = int(policy_logits.argmax())
        policy_action_regrets.append(max(0.0, teacher_best - float(teacher_values[selected])))
        policy_expected_regrets.append(
            max(0.0, teacher_best - float(np.dot(predicted, teacher_values)))
        )
        policy_top1_matches.append(selected == int(teacher_values.argmax()))
    confidence = probability.max(axis=1)
    correct = probability.argmax(axis=1) == y[indices].argmax(axis=1)
    reliability = []
    for lower in np.linspace(0.0, 0.9, 10):
        upper = lower + 0.1
        members = (confidence >= lower) & (
            confidence <= upper if upper >= 1.0 else confidence < upper
        )
        reliability.append(
            {
                "lower": float(lower),
                "upper": float(upper),
                "samples": int(members.sum()),
                "meanConfidence": (
                    float(confidence[members].mean()) if members.any() else None
                ),
                "accuracy": float(correct[members].mean()) if members.any() else None,
            }
        )

    def grouped_metrics(key: str) -> dict:
        buckets: dict[str, list[int]] = {}
        for local_index, source_index in enumerate(indices):
            value = data[int(source_index)].get(key, "unknown")
            buckets.setdefault(str(value), []).append(local_index)
        output: dict[str, dict] = {}
        for value, members_list in sorted(buckets.items()):
            members = np.asarray(members_list, dtype=np.int64)
            if len(members) < 5:
                continue
            bucket_probability = probability[members]
            bucket_target = y[indices][members]
            output[value] = {
                "samples": int(len(members)),
                "brier": float(
                    np.sum((bucket_probability - bucket_target) ** 2, axis=1).mean()
                ),
                "logLoss": float(
                    -np.sum(
                        bucket_target * np.log(bucket_probability.clip(1e-8)),
                        axis=1,
                    ).mean()
                ),
            }
        return output

    return {
        "valueBrier": brier,
        "valueLogLoss": log_loss,
        "valueTop1Accuracy": accuracy,
        "uniformValueBrier": uniform_brier,
        "uniformValueLogLoss": uniform_log_loss,
        "temperature": temperature,
        "policyCrossEntropy": float(np.mean(policy_losses)) if policy_losses else None,
        "uniformPolicyCrossEntropy": (
            float(np.mean(uniform_policy_losses))
            if uniform_policy_losses
            else None
        ),
        "policyMeanActionRegret": (
            float(np.mean(policy_action_regrets)) if policy_action_regrets else None
        ),
        "policyMeanExpectedActionRegret": (
            float(np.mean(policy_expected_regrets)) if policy_expected_regrets else None
        ),
        "policyTeacherTop1Agreement": (
            float(np.mean(policy_top1_matches)) if policy_top1_matches else None
        ),
        "reliability": reliability,
        "calibrationByPhase": grouped_metrics("phase"),
        "calibrationByVictoryPoints": grouped_metrics("actorVictoryPoints"),
    }


def rust_array(name: str, values: np.ndarray) -> str:
    flattened = values.astype(np.float32).ravel()
    rows = []
    for start in range(0, len(flattened), 8):
        rows.append(
            "    "
            + ", ".join(f"{float(value):.9g}f32" for value in flattened[start : start + 8])
            + ","
        )
    return f"pub const {name}: &[f32] = &[\n" + "\n".join(rows) + "\n];"


def main() -> None:
    args = arguments()
    # Fail an invalid unforced configuration early, before any file I/O or
    # model training: a Task 15 checkpoint screen must compare against the
    # matched old 48-action-feature baseline. --force remains the explicit
    # route for an unsupported, unpromoted checkpoint.
    task15_baseline_requested = is_task15_baseline(
        args.baseline_state_features, args.baseline_action_features
    )
    if not args.force and not task15_baseline_requested:
        raise SystemExit(
            "Task 15 checkpoint screen requires unchanged state features and "
            "--baseline-action-features 48; "
            f"got state={args.baseline_state_features}, "
            f"action={args.baseline_action_features}. Re-run with the matched baseline "
            "or pass --force to write an explicitly unsupported, unpromoted checkpoint."
        )
    raw_data = records(args.inputs)
    if args.max_samples > 0:
        raw_data = raw_data[: args.max_samples]
    raw_schema_versions = sorted(
        {int(record.get("schemaVersion", 1)) for record in raw_data}
    )
    if raw_schema_versions != [2]:
        raise SystemExit(
            f"Task 15 training requires strategic feature schema 2, got {raw_schema_versions}"
        )
    raw_teacher_engines = sorted({str(record.get("engine", "")) for record in raw_data})
    allowed_teacher_engines = {"puct-teacher", "native-gpu-teacher"}
    if len(raw_teacher_engines) != 1 or raw_teacher_engines[0] not in allowed_teacher_engines:
        raise SystemExit(
            "Task 15 training requires a uniform single-teacher corpus "
            "('puct-teacher' or 'native-gpu-teacher'); "
            f"got {raw_teacher_engines}. Refusing to mix incompatible policy semantics."
        )
    teacher_engine = raw_teacher_engines[0]
    if teacher_engine != "puct-teacher":
        raise SystemExit(
            "Task 15 checkpoint training requires puct-teacher value semantics. "
            "native-gpu-teacher records are policy-only screening evidence because "
            "their rootValue/searchValue fields are rollout VP diagnostics, not "
            "normalized strategic-value targets. Use benchmark-gpu-zoom.py for the "
            "native-GPU policy feature screen."
        )
    data = [record for record in raw_data if has_usable_teacher_value(record)]
    invalid_teacher_samples = len(raw_data) - len(data)
    if len(data) < 50:
        raise SystemExit("Need at least 50 usable expert teacher samples")
    digest = hashlib.sha256()
    for path in args.inputs:
        digest.update(path.read_bytes())
    version = f"expert-iteration-{digest.hexdigest()[:12]}"
    rng = np.random.default_rng(args.seed)
    state_widths = sorted({len(record["stateFeatures"]) for record in data})
    if len(state_widths) != 1:
        raise SystemExit(f"Expert data mixes state feature widths: {state_widths}")
    x = np.asarray([record["stateFeatures"] for record in data], dtype=np.float32)
    if not np.isfinite(x).all():
        raise SystemExit("Expert state features contain NaN or infinite values")
    if args.baseline_state_features < 0 or args.baseline_state_features > x.shape[1]:
        raise SystemExit("--baseline-state-features must be within the state feature width")
    action_widths = sorted(
        {len(action["features"]) for record in data for action in record.get("actions", [])}
    )
    if len(action_widths) != 1 or not is_task15_action_width(action_widths[0]):
        raise SystemExit(
            "Task 15 schema-2 expert data requires exactly 52 action features, "
            f"got {action_widths}"
        )
    if (
        args.baseline_action_features < 0
        or args.baseline_action_features > action_widths[0]
    ):
        raise SystemExit("--baseline-action-features must be within the action feature width")
    y = np.asarray([relative_teacher_value(record) for record in data], dtype=np.float32)
    players = np.asarray([record["players"] for record in data], dtype=np.int64)
    state_groups = [
        f'{record.get("boardSeed", "unknown")}:{record.get("chanceSeed", "unknown")}'
        for record in data
    ]
    held_out_groups = validation_groups(state_groups, args.validation_fraction)
    held_out = np.asarray(
        [group in held_out_groups for group in state_groups],
        dtype=bool,
    )
    train_indices = np.flatnonzero(~held_out)
    validation_indices = np.flatnonzero(held_out)
    value_weights = train_value(x, y, players, train_indices, args, rng)

    groups = policy_groups(data)
    if not groups:
        raise SystemExit("Expert data contains no searched action groups")
    policy_widths = sorted({group[0].shape[1] for group in groups})
    if len(policy_widths) != 1:
        raise SystemExit(f"Expert data mixes policy feature widths: {policy_widths}")
    if any(
        not np.isfinite(group[0]).all()
        or not np.isfinite(group[1]).all()
        or not np.isfinite(group[3]).all()
        for group in groups
    ):
        raise SystemExit("Expert policy features or teacher targets contain NaN or infinite values")
    group_validation = np.asarray(
        [group[2] in held_out_groups for group in groups],
        dtype=bool,
    )
    policy_train = np.flatnonzero(~group_validation)
    policy_validation = np.flatnonzero(group_validation)
    if not len(policy_train) or not len(policy_validation):
        raise SystemExit(
            "Grouped policy split must contain both training and validation games"
        )
    policy_weights = train_policy(groups, policy_train, args, rng)
    training_metrics = validation_metrics(
        x,
        y,
        players,
        train_indices,
        value_weights,
        groups,
        policy_train,
        policy_weights,
        data,
    )
    metrics = validation_metrics(
        x,
        y,
        players,
        validation_indices,
        value_weights,
        groups,
        policy_validation,
        policy_weights,
        data,
        temperature=training_metrics["temperature"],
    )
    baseline_metrics = None
    baseline_training_metrics = None
    if args.baseline_state_features > 0 or args.baseline_action_features > 0:
        baseline_x = x.copy()
        if args.baseline_state_features > 0:
            baseline_x[:, args.baseline_state_features :] = 0.0
        baseline_groups = policy_groups(
            data, args.baseline_state_features, args.baseline_action_features
        )
        baseline_rng = np.random.default_rng(args.seed)
        baseline_value_weights = train_value(
            baseline_x, y, players, train_indices, args, baseline_rng
        )
        baseline_policy_weights = train_policy(
            baseline_groups, policy_train, args, baseline_rng
        )
        baseline_training_metrics = validation_metrics(
            baseline_x,
            y,
            players,
            train_indices,
            baseline_value_weights,
            baseline_groups,
            policy_train,
            baseline_policy_weights,
            data,
        )
        baseline_metrics = validation_metrics(
            baseline_x,
            y,
            players,
            validation_indices,
            baseline_value_weights,
            baseline_groups,
            policy_validation,
            baseline_policy_weights,
            data,
            temperature=baseline_training_metrics["temperature"],
        )
    feature_comparison = None
    # A missing baseline is not evaluated, not passed: an unforced checkpoint
    # screen requires the matched Task 15 ablation (--baseline-action-features).
    feature_ablation_passed = False
    baseline_evaluated = task15_baseline_requested and baseline_metrics is not None
    if baseline_metrics is not None:
        tolerance = 1e-6
        candidate_action_regret = metrics["policyMeanActionRegret"]
        baseline_action_regret = baseline_metrics["policyMeanActionRegret"]
        candidate_expected_regret = metrics["policyMeanExpectedActionRegret"]
        baseline_expected_regret = baseline_metrics["policyMeanExpectedActionRegret"]
        action_regret_passed = (
            candidate_action_regret is not None
            and baseline_action_regret is not None
            and candidate_action_regret <= baseline_action_regret + tolerance
        )
        expected_regret_passed = (
            candidate_expected_regret is not None
            and baseline_expected_regret is not None
            and candidate_expected_regret <= baseline_expected_regret + tolerance
        )
        policy_cross_entropy_passed = (
            metrics["policyCrossEntropy"] is not None
            and baseline_metrics["policyCrossEntropy"] is not None
            and metrics["policyCrossEntropy"]
            <= baseline_metrics["policyCrossEntropy"] + tolerance
        )
        value_log_loss_passed = (
            metrics["valueLogLoss"] <= baseline_metrics["valueLogLoss"] + tolerance
        )
        value_brier_passed = (
            metrics["valueBrier"] <= baseline_metrics["valueBrier"] + tolerance
        )
        feature_ablation_passed = task15_feature_ablation_passed(
            baseline_evaluated,
            (
                action_regret_passed,
                expected_regret_passed,
                policy_cross_entropy_passed,
                value_log_loss_passed,
                value_brier_passed,
            ),
        )
        feature_comparison = {
            "policyActionRegretImprovement": (
                baseline_action_regret - candidate_action_regret
                if baseline_action_regret is not None and candidate_action_regret is not None
                else None
            ),
            "policyExpectedRegretImprovement": (
                baseline_expected_regret - candidate_expected_regret
                if baseline_expected_regret is not None
                and candidate_expected_regret is not None
                else None
            ),
            "policyCrossEntropyImprovement": (
                baseline_metrics["policyCrossEntropy"] - metrics["policyCrossEntropy"]
                if baseline_metrics["policyCrossEntropy"] is not None
                and metrics["policyCrossEntropy"] is not None
                else None
            ),
            "valueLogLossImprovement": (
                baseline_metrics["valueLogLoss"] - metrics["valueLogLoss"]
            ),
            "valueBrierImprovement": baseline_metrics["valueBrier"] - metrics["valueBrier"],
            "policyTeacherTop1AgreementImprovement": (
                metrics["policyTeacherTop1Agreement"]
                - baseline_metrics["policyTeacherTop1Agreement"]
                if metrics["policyTeacherTop1Agreement"] is not None
                and baseline_metrics["policyTeacherTop1Agreement"] is not None
                else None
            ),
            "actionRegretPassed": bool(action_regret_passed),
            "expectedRegretPassed": bool(expected_regret_passed),
            "policyCrossEntropyPassed": bool(policy_cross_entropy_passed),
            "valueLogLossPassed": bool(value_log_loss_passed),
            "valueBrierPassed": bool(value_brier_passed),
            "accepted": bool(feature_ablation_passed),
        }

    training_group_keys = sorted(set(state_groups) - held_out_groups)
    validation_group_keys = sorted(held_out_groups)
    metrics.update(
        {
            "schemaVersion": 2,
            "modelVersion": version,
            "teacherEngine": teacher_engine,
            "samples": len(data),
            "invalidTeacherSamplesSkipped": invalid_teacher_samples,
            "trainingSamples": len(train_indices),
            "validationSamples": len(validation_indices),
            "trainingStateGroups": len(training_group_keys),
            "validationStateGroups": len(validation_group_keys),
            "trainingGroupKeys": training_group_keys,
            "validationGroupKeys": validation_group_keys,
            "policyGroups": len(groups),
            "stateFeatures": int(x.shape[1]),
            "actionFeatures": int(groups[0][0].shape[1] - x.shape[1]),
            "baselineStateFeatures": (
                args.baseline_state_features if args.baseline_state_features > 0 else None
            ),
            "baselineActionFeatures": (
                args.baseline_action_features if args.baseline_action_features > 0 else None
            ),
            "hidden": args.hidden,
            "seed": args.seed,
            "trainingMetrics": training_metrics,
            "baselineHeldOutMetrics": baseline_metrics,
            "baselineTrainingMetrics": baseline_training_metrics,
            "featureComparison": feature_comparison,
        }
    )
    validation_support_passed = (
        metrics["validationStateGroups"] >= args.minimum_validation_groups
    )
    value_log_loss_gain = (
        metrics["uniformValueLogLoss"] - metrics["valueLogLoss"]
    )
    value_brier_gain = (
        metrics["uniformValueBrier"] - metrics["valueBrier"]
    )
    value_passed = (
        validation_support_passed
        and value_log_loss_gain >= args.minimum_value_log_loss_gain
        and value_brier_gain >= args.minimum_value_brier_gain
    )
    policy_cross_entropy_gain = (
        metrics["uniformPolicyCrossEntropy"] - metrics["policyCrossEntropy"]
        if metrics["policyCrossEntropy"] is not None
        and metrics["uniformPolicyCrossEntropy"] is not None
        else None
    )
    policy_beats_uniform = (
        policy_cross_entropy_gain is not None
        and policy_cross_entropy_gain >= args.minimum_policy_cross_entropy_gain
    )
    policy_passed = validation_support_passed and policy_beats_uniform
    predictive_quality_passed = value_passed and policy_passed
    checkpoint_screen_passed = predictive_quality_passed and feature_ablation_passed
    metrics["promotionGate"] = {
        "minimumValidationGroups": args.minimum_validation_groups,
        "validationSupportPassed": bool(validation_support_passed),
        "valueLogLossGain": value_log_loss_gain,
        "minimumValueLogLossGain": args.minimum_value_log_loss_gain,
        "valueBrierGain": value_brier_gain,
        "minimumValueBrierGain": args.minimum_value_brier_gain,
        "valuePassed": bool(value_passed),
        "policyCrossEntropyGain": policy_cross_entropy_gain,
        "minimumPolicyCrossEntropyGain": args.minimum_policy_cross_entropy_gain,
        "policyBeatsUniform": bool(policy_beats_uniform),
        "policyPassed": bool(policy_passed),
        "predictiveQualityPassed": bool(predictive_quality_passed),
        "baselineEvaluated": bool(baseline_evaluated),
        "featureAblationPassed": bool(feature_ablation_passed),
        "checkpointScreenPassed": bool(checkpoint_screen_passed),
        "matchedArenaRequired": True,
        "matchedArenaSatisfied": False,
        "accepted": False,
        "forced": bool(args.force),
    }
    args.metrics.parent.mkdir(parents=True, exist_ok=True)
    args.metrics.write_text(json.dumps(metrics, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(metrics, indent=2))
    if not args.force and not checkpoint_screen_passed:
        raise SystemExit(
            "checkpoint rejected: held-out predictive and matched feature-ablation gates must pass"
        )

    vw1, vb1, vw2, vb2 = value_weights
    calibration_temperature = float(metrics["temperature"])
    vw2 = vw2 / calibration_temperature
    vb2 = vb2 / calibration_temperature
    pw1, pb1, pw2, pb2 = policy_weights
    source = "\n".join(
        [
            "// Generated by scripts/train-strategic-model.py. Do not edit by hand.",
            f'pub const MODEL_VERSION: &str = "{version}";',
            "pub const STRATEGIC_FEATURE_SCHEMA_VERSION: u8 = 2;",
            "pub const VALUE_MODEL_PROMOTED: bool = false;",
            "pub const POLICY_MODEL_PROMOTED: bool = false;",
            f"pub const VALUE_HIDDEN: usize = {args.hidden};",
            f"pub const POLICY_HIDDEN: usize = {args.hidden};",
            rust_array("VALUE_W1", vw1),
            rust_array("VALUE_B1", vb1),
            rust_array("VALUE_W2", vw2),
            rust_array("VALUE_B2", vb2),
            rust_array("POLICY_W1", pw1),
            rust_array("POLICY_B1", pb1),
            rust_array("POLICY_W2", pw2),
            f"pub const POLICY_B2: f32 = {pb2:.9g}f32;",
            "",
        ]
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(source, encoding="utf-8")


if __name__ == "__main__":
    main()
