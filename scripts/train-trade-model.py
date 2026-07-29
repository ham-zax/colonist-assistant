#!/usr/bin/env python3
"""Train and validate the compact Catan trade-acceptance model."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path

import numpy as np


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("inputs", nargs="+", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("engine/crates/catan-search/src/trade_model_weights.rs"),
    )
    parser.add_argument("--epochs", type=int, default=1200)
    parser.add_argument("--learning-rate", type=float, default=0.08)
    parser.add_argument("--l2", type=float, default=0.002)
    parser.add_argument("--seed", type=int, default=20260728)
    parser.add_argument(
        "--engines",
        default="puct,maxn,alphabeta,uct,weighted",
        help="Comma-separated teacher policies retained from arena logs.",
    )
    parser.add_argument("--force", action="store_true")
    return parser.parse_args()


def load(
    paths: list[Path], engines: set[str]
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    samples = [
        json.loads(line)
        for path in paths
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    samples = [
        sample
        for sample in samples
        if str(sample.get("engine", "")).lower() in engines
    ]
    if len(samples) < 100:
        raise SystemExit("at least 100 trade-response samples are required")
    features = np.asarray([sample["features"] for sample in samples], dtype=np.float64)
    labels = np.asarray([float(sample["accepted"]) for sample in samples], dtype=np.float64)
    groups = np.asarray(
        [
            f'{sample.get("boardSeed", "unknown")}:{sample.get("chanceSeed", "unknown")}'
            for sample in samples
        ]
    )
    if features.ndim != 2 or not np.isfinite(features).all():
        raise SystemExit("trade feature rows must be finite and equally sized")
    if labels.min() == labels.max():
        raise SystemExit("trade data must contain both accepts and non-accepts")
    return features, labels, groups


def sigmoid(value: np.ndarray) -> np.ndarray:
    return 1.0 / (1.0 + np.exp(-np.clip(value, -30.0, 30.0)))


def metrics(probability: np.ndarray, labels: np.ndarray) -> tuple[float, float]:
    clipped = np.clip(probability, 1e-7, 1.0 - 1e-7)
    brier = float(np.mean((clipped - labels) ** 2))
    log_loss = float(
        -np.mean(labels * np.log(clipped) + (1.0 - labels) * np.log(1.0 - clipped))
    )
    return brier, log_loss


def rust_array(values: np.ndarray) -> str:
    return ", ".join(f"{float(value):.9g}_f32" for value in values)


def main() -> None:
    args = parse_args()
    selected_engines = {
        engine.strip().lower()
        for engine in args.engines.split(",")
        if engine.strip()
    }
    x, y, groups = load(args.inputs, selected_engines)
    rng = np.random.default_rng(args.seed)
    unique_groups = np.unique(groups)
    if len(unique_groups) < 2:
        raise SystemExit(
            "at least two independent board/chance seed groups are required"
        )
    shuffled_groups = rng.permutation(unique_groups)
    split = min(
        len(shuffled_groups) - 1,
        max(1, int(len(shuffled_groups) * 0.8)),
    )
    training_groups = set(shuffled_groups[:split])
    train = np.flatnonzero(
        np.asarray([group in training_groups for group in groups], dtype=bool)
    )
    validation = np.flatnonzero(
        np.asarray([group not in training_groups for group in groups], dtype=bool)
    )
    if not len(train) or not len(validation):
        raise SystemExit(
            "grouped trade split must contain both training and validation games"
        )

    weights = np.zeros(x.shape[1], dtype=np.float64)
    bias = math.log((float(y[train].mean()) + 1e-4) / (1.0001 - float(y[train].mean())))
    for _ in range(args.epochs):
        probability = sigmoid(x[train] @ weights + bias)
        residual = probability - y[train]
        weights -= args.learning_rate * (
            x[train].T @ residual / len(train) + args.l2 * weights
        )
        bias -= args.learning_rate * float(residual.mean())

    probability = sigmoid(x[validation] @ weights + bias)
    brier, log_loss = metrics(probability, y[validation])
    base_probability = np.full(len(validation), float(y[train].mean()))
    base_brier, base_log_loss = metrics(base_probability, y[validation])
    print(
        json.dumps(
            {
                "samples": len(y),
                "features": x.shape[1],
                "validationSamples": len(validation),
                "acceptRate": float(y.mean()),
                "engines": sorted(selected_engines),
                "trainingStateGroups": len(set(groups[train])),
                "validationStateGroups": len(set(groups[validation])),
                "brier": brier,
                "logLoss": log_loss,
                "baseBrier": base_brier,
                "baseLogLoss": base_log_loss,
            },
            indent=2,
        )
    )
    if not args.force and not (log_loss < base_log_loss and brier <= base_brier):
        raise SystemExit("checkpoint rejected: held-out calibration did not beat the base rate")

    digest_source = hashlib.sha256()
    for path in args.inputs:
        digest_source.update(path.read_bytes())
    digest = digest_source.hexdigest()[:12]
    content = "\n".join(
        [
            "// Generated by scripts/train-trade-model.py.",
            f'pub const MODEL_VERSION: &str = "trade-{digest}";',
            f"pub const WEIGHTS: &[f32] = &[{rust_array(weights)}];",
            f"pub const BIAS: f32 = {bias:.9g}_f32;",
            "",
        ]
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(content, encoding="utf-8")


if __name__ == "__main__":
    main()
