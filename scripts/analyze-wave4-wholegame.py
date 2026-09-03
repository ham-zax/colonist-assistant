#!/usr/bin/env python3
"""Summarize Wave 4 whole-game searched-agent evidence by matched block."""

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path
from typing import Iterable


def percentile(values: list[float], probability: float) -> float:
    ordered = sorted(values)
    if not ordered:
        raise ValueError("cannot take percentile of an empty sample")
    position = probability * (len(ordered) - 1)
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = position - lower
    return ordered[lower] * (1.0 - fraction) + ordered[upper] * fraction


def bootstrap_intervals(
    blocks: list[dict], players: int, samples: int, seed: int
) -> dict[str, list[float]]:
    if not blocks:
        raise ValueError("benchmark result has no matched blocks")
    rng = random.Random(seed)
    block_count = len(blocks)
    fair_share = 1.0 / players
    strength: list[float] = []
    margins: list[float] = []
    for _ in range(samples):
        wins = 0
        margin = 0.0
        for _ in range(block_count):
            block = blocks[rng.randrange(block_count)]
            wins += block["candidateWins"]
            margin += block["meanCandidateMargin"]
        strength.append(wins / (block_count * players) - fair_share)
        margins.append(margin / block_count)
    return {
        "strengthDelta": [percentile(strength, 0.025), percentile(strength, 0.975)],
        "meanVpMargin": [percentile(margins, 0.025), percentile(margins, 0.975)],
    }


def sum_field(blocks: Iterable[dict], field: str) -> int:
    return sum(int(block[field]) for block in blocks)


def summarize(path: Path, samples: int, seed: int) -> dict:
    payload = json.loads(path.read_text())
    results = payload.get("results", [])
    if len(results) != 1:
        raise ValueError(f"{path}: expected exactly one player-count result")
    result = results[0]
    config = payload["config"]
    players = int(result["players"])
    blocks = result["matchedBlocks"]
    fair_share = 1.0 / players
    terminal_games = int(result["terminalGames"])
    games = int(result["games"])
    wins = int(result["candidateWins"])
    truncations = {
        "total": sum_field(blocks, "totalTruncations"),
        "candidateSeat": sum_field(blocks, "candidateSeatTruncations"),
        "opponentSeat": sum_field(blocks, "opponentSeatTruncations"),
        "unattributed": sum_field(blocks, "unattributedTruncations"),
    }
    decisions = int(result["candidateDecisions"])
    proposals = int(result["rootProposalsEvaluated"])
    outcome_counts = {"win": 0, "loss": 0, "truncated": 0}
    for block in blocks:
        for outcome in block["candidateSeatOutcomes"]:
            outcome_counts[outcome["outcome"]] += 1
    return {
        "source": str(path),
        "players": players,
        "tradeStressMode": config.get(
            "tradeStressMode",
            "enabled" if config.get("playerTradesEnabled", True) else "global_disabled",
        ),
        "search": config["search"],
        "blocks": int(result["blocks"]),
        "candidateSeatGames": games,
        "terminalGames": terminal_games,
        "candidateWins": wins,
        "candidateWinRate": wins / terminal_games if terminal_games else 0.0,
        "candidateWinRateAllGames": wins / games if games else 0.0,
        "theoreticalFairShare": fair_share,
        "matchedStrengthDelta": wins / games - fair_share if games else -fair_share,
        "meanCandidateVp": float(result["meanCandidateVictoryPoints"]),
        "meanBestOpponentVp": float(result["meanBestOpponentVictoryPoints"]),
        "meanVpMargin": float(result["meanVictoryMargin"]),
        "blockBootstrap95": bootstrap_intervals(blocks, players, samples, seed),
        "truncations": truncations,
        "candidateDecisions": decisions,
        "rootProposalsEvaluated": proposals,
        "rootProposalsPerDecision": proposals / decisions if decisions else 0.0,
        "totalActions": int(result["totalActions"]),
        "meanActions": float(result["meanActions"]),
        "elapsedMs": float(result["elapsedMs"]),
        "outcomes": outcome_counts,
        "rankAvailable": False,
        "mechanismMetricsAvailableInGpuBenchmark": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reports", nargs="+", type=Path)
    parser.add_argument("--bootstrap-samples", type=int, default=50_000)
    parser.add_argument("--bootstrap-seed", type=int, default=20260903)
    args = parser.parse_args()
    if args.bootstrap_samples <= 0:
        parser.error("--bootstrap-samples must be positive")
    summaries = [
        summarize(path, args.bootstrap_samples, args.bootstrap_seed + index)
        for index, path in enumerate(args.reports)
    ]
    json.dump(summaries, fp=__import__("sys").stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
