#!/usr/bin/env python3
"""Run a seat-balanced reference benchmark inside an upstream Catanatron checkout.

This intentionally does not call its results "Colonist Assistant" results:
Catanatron has a different state/action model and cannot load the extension's
browser policy without a full rules adapter. The output is calibration evidence
for upstream AlphaBeta and heuristic opponents, not a fake cross-engine port.
"""

from __future__ import annotations

import argparse
import json
import math
import random
import subprocess
import sys
import time
from concurrent.futures import ProcessPoolExecutor
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass(frozen=True)
class Task:
    source: str
    players: int
    block: int
    seat: int
    seed: int
    candidate: str
    baseline: str


def play(task: Task) -> dict[str, int | bool]:
    source = Path(task.source)
    sys.path.insert(0, str(source / "catanatron"))
    from catanatron.cli.cli_players import parse_cli_string
    from catanatron.cli.play import GameConfigOptions
    from catanatron.game import Game
    from catanatron.models.map import build_map

    random.seed(task.seed)
    codes = [task.baseline] * task.players
    codes[task.seat] = task.candidate
    players = parse_cli_string(",".join(codes))
    if len(players) != task.players:
        raise RuntimeError(f"Could not parse player codes: {codes}")
    for player in players:
        player.reset_state()
    config = GameConfigOptions(map_type="BASE", number_placement="official_spiral")
    game = Game(
        players,
        discard_limit=config.discard_limit,
        friendly_robber=config.friendly_robber,
        vps_to_win=config.vps_to_win,
        catan_map=build_map(config.map_type, config.number_placement),
    )
    game.play()
    return {
        "block": task.block,
        "seat": task.seat,
        "won": game.winning_color() == players[task.seat].color,
        "turns": game.state.num_turns,
    }


def percentile(values: list[float], probability: float) -> float:
    return sorted(values)[round((len(values) - 1) * probability)]


def bootstrap(scores: list[float], seed: int) -> tuple[float, float]:
    rng = random.Random(seed)
    samples = [
        sum(rng.choice(scores) for _ in scores) / len(scores) for _ in range(2000)
    ]
    return percentile(samples, 0.025), percentile(samples, 0.975)


def git_revision(source: Path) -> str:
    return subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, help="Catanatron checkout")
    parser.add_argument("--players", default="2,3,4")
    parser.add_argument("--games", type=int, default=60)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--candidate", default="AB:2")
    parser.add_argument("--baselines", default="R,W,F")
    parser.add_argument("--seed", type=int, default=92000001)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    source = Path(args.source).resolve()
    players_values = [int(value) for value in args.players.split(",")]
    baselines = [value.strip() for value in args.baselines.split(",") if value.strip()]
    if not (source / "catanatron" / "catanatron").is_dir():
        raise SystemExit("--source is not a Catanatron checkout")
    if any(value not in (2, 3, 4) for value in players_values):
        raise SystemExit("--players supports 2, 3, and 4")

    started = time.perf_counter()
    results = []
    for player_count in players_values:
        for baseline_index, baseline in enumerate(baselines):
            blocks = math.ceil(args.games / player_count)
            tasks = [
                Task(
                    str(source),
                    player_count,
                    block,
                    seat,
                    args.seed
                    + player_count * 1_000_000
                    + baseline_index * 100_000
                    + block,
                    args.candidate,
                    baseline,
                )
                for block in range(blocks)
                for seat in range(player_count)
            ]
            print(
                f"[Catanatron] {args.candidate} vs {baseline}, "
                f"{player_count} players, {len(tasks)} games",
                file=sys.stderr,
            )
            with ProcessPoolExecutor(max_workers=args.jobs) as executor:
                games = list(executor.map(play, tasks))
            wins = sum(bool(game["won"]) for game in games)
            block_scores = [
                sum(
                    bool(game["won"])
                    for game in games
                    if game["block"] == block
                )
                / player_count
                for block in range(blocks)
            ]
            lower, upper = bootstrap(
                block_scores, args.seed ^ player_count ^ baseline_index
            )
            results.append(
                {
                    "simulator": "catanatron-upstream-reference",
                    "candidate": args.candidate,
                    "baseline": baseline,
                    "players": player_count,
                    "blocks": blocks,
                    "games": len(games),
                    "candidateWins": wins,
                    "winShare": wins / len(games),
                    "fairShare": 1 / player_count,
                    "blockedCi95": {"lower": lower, "upper": upper},
                    "meanTurns": sum(int(game["turns"]) for game in games)
                    / len(games),
                }
            )
            print(
                f"  {wins}/{len(games)} = {wins / len(games):.1%}",
                file=sys.stderr,
            )

    report = {
        "schemaVersion": 1,
        "kind": "external-reference-not-colonist-assistant",
        "warning": (
            "These are upstream Catanatron policy results. They are not direct "
            "Colonist Assistant results and must not be relabelled as such."
        ),
        "source": {
            "repository": "https://github.com/bcollazo/catanatron",
            "checkout": str(source),
            "revision": git_revision(source),
        },
        "configuration": {
            "players": players_values,
            "minimumGamesPerMatchup": args.games,
            "jobs": args.jobs,
            "candidate": args.candidate,
            "baselines": baselines,
            "seed": args.seed,
        },
        "elapsedSeconds": time.perf_counter() - started,
        "totalGames": sum(result["games"] for result in results),
        "results": results,
    }
    output = Path(args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"json": str(output), "totalGames": report["totalGames"]}))


if __name__ == "__main__":
    main()
