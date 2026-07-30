#!/usr/bin/env python3
"""Train the compact graph-readout value and policy models.

Input is JSONL emitted by `colonist-arena --expert-output`. The arena records
PUCT visit distributions and terminal winners; this script performs deterministic
Expert Iteration training and writes Rust constants consumed by the WASM engine.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Iterable

import numpy as np


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
        "--force",
        action="store_true",
        help="Write a checkpoint even when held-out gates fail.",
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


def relative_outcome(record: dict) -> np.ndarray:
    players = int(record["players"])
    actor = int(record["actor"])
    canonical = record["outcome"]
    target = np.zeros(4, dtype=np.float32)
    for offset in range(players):
        target[offset] = canonical[(actor + offset) % players]
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
) -> list[tuple[np.ndarray, np.ndarray, str]]:
    groups: list[tuple[np.ndarray, np.ndarray, str]] = []
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
        state = np.asarray(record["stateFeatures"], dtype=np.float32)
        features = np.asarray(
            [np.concatenate((state, action["features"])) for action in actions],
            dtype=np.float32,
        )
        target = np.asarray([action["policy"] for action in actions], dtype=np.float32)
        target /= target.sum().clip(1e-8)
        groups.append(
            (
                features,
                target,
                f'{record.get("boardSeed", "unknown")}:{record.get("chanceSeed", "unknown")}',
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
    groups: list[tuple[np.ndarray, np.ndarray, str]],
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
            features, target, _ = groups[int(index)]
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
    groups: list[tuple[np.ndarray, np.ndarray, str]],
    policy_indices: np.ndarray,
    policy_weights: tuple[np.ndarray, np.ndarray, np.ndarray, float],
    data: list[dict],
) -> dict:
    w1, b1, w2, b2 = value_weights
    hidden = np.maximum(x[indices] @ w1.T + b1, 0)
    logits = hidden @ w2.T + b2
    # Temperature calibration is selected only on held-out records.
    candidates = np.linspace(0.65, 2.5, 75, dtype=np.float32)
    losses = []
    for temperature in candidates:
        probability = softmax(logits / temperature, players[indices])
        losses.append(
            -np.sum(y[indices] * np.log(probability.clip(1e-8)), axis=1).mean()
        )
    temperature = float(candidates[int(np.argmin(losses))])
    w2 /= temperature
    b2 /= temperature
    probability = softmax(hidden @ w2.T + b2, players[indices])
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
    for index in policy_indices:
        features, target, _ = groups[int(index)]
        policy_logits = np.maximum(features @ pw1.T + pb1, 0) @ pw2 + pb2
        policy_logits -= policy_logits.max()
        predicted = np.exp(policy_logits)
        predicted /= predicted.sum().clip(1e-8)
        policy_losses.append(float(-(target * np.log(predicted.clip(1e-8))).sum()))
        uniform_policy_losses.append(float(np.log(max(1, len(target)))))
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
    data = [
        record
        for record in records(args.inputs)
        if bool(record.get("terminal", sum(record.get("outcome", [])) > 0))
        and abs(sum(record.get("outcome", [])) - 1.0) < 1e-5
    ]
    if args.max_samples > 0:
        data = data[: args.max_samples]
    if len(data) < 50:
        raise SystemExit("Need at least 50 expert samples")
    digest = hashlib.sha256()
    for path in args.inputs:
        digest.update(path.read_bytes())
    version = f"expert-iteration-{digest.hexdigest()[:12]}"
    rng = np.random.default_rng(args.seed)
    x = np.asarray([record["stateFeatures"] for record in data], dtype=np.float32)
    y = np.asarray([relative_outcome(record) for record in data], dtype=np.float32)
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
    )
    metrics.update(
        {
            "schemaVersion": 1,
            "modelVersion": version,
            "samples": len(data),
            "trainingSamples": len(train_indices),
            "validationSamples": len(validation_indices),
            "trainingStateGroups": len(set(state_groups) - held_out_groups),
            "validationStateGroups": len(held_out_groups),
            "policyGroups": len(groups),
            "stateFeatures": int(x.shape[1]),
            "actionFeatures": int(groups[0][0].shape[1] - x.shape[1]),
            "hidden": args.hidden,
            "seed": args.seed,
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
        "accepted": bool(value_passed and policy_passed),
        "forced": bool(args.force),
    }
    args.metrics.parent.mkdir(parents=True, exist_ok=True)
    args.metrics.write_text(json.dumps(metrics, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(metrics, indent=2))
    if not args.force and not (value_passed and policy_passed):
        raise SystemExit(
            "checkpoint rejected: held-out value and policy must both beat uniform baselines"
        )

    vw1, vb1, vw2, vb2 = value_weights
    pw1, pb1, pw2, pb2 = policy_weights
    source = "\n".join(
        [
            "// Generated by scripts/train-strategic-model.py. Do not edit by hand.",
            f'pub const MODEL_VERSION: &str = "{version}";',
            f"pub const VALUE_MODEL_PROMOTED: bool = {'true' if value_passed else 'false'};",
            f"pub const POLICY_MODEL_PROMOTED: bool = {'true' if policy_passed else 'false'};",
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
