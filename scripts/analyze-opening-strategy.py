#!/usr/bin/env python3
"""Analyze a frozen opening/strategy arena cohort.

The analyzer is intentionally standard-library only. It compares the MaxN
candidate's complete two-settlement production and early executed build shape
against terminal game outcomes.
"""

from __future__ import annotations

import argparse
import gzip
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import TextIO

PROFILE_NAMES = {
    (51, 51, 51, 51, 51): "neutral",
    (45, 92, 28, 38, 45): "road-heavy",
    (45, 30, 92, 35, 45): "dev-heavy",
    (45, 45, 45, 96, 25): "trade-happy",
    (45, 45, 45, 15, 96): "trade-resistant",
    (70, 55, 55, 55, 55): "flex-general",
}


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("checkpoints", type=Path)
    parser.add_argument("trajectories", type=Path)
    parser.add_argument("--horizon", type=int, default=48)
    return parser.parse_args()


def open_text(path: Path) -> TextIO:
    if path.suffix == ".gz":
        return gzip.open(path, "rt", encoding="utf-8")
    return path.open(encoding="utf-8")


def mean(group: list[dict], field: str) -> float:
    return statistics.mean(row[field] for row in group)


def main() -> None:
    args = arguments()
    checkpoints: dict[tuple[int, int], dict] = {}
    with open_text(args.checkpoints) as handle:
        for line in handle:
            record = json.loads(line)
            game = record["lastGame"]
            key = (game["block"], game["rotation"])
            if "maxn" not in game["engines"]:
                continue
            checkpoints[key] = {
                "candidate": game["engines"].index("maxn"),
                "game": game,
            }

    setup: dict[tuple[int, int], dict] = {}
    horizon: dict[tuple[int, int], dict] = {}
    final: dict[tuple[int, int], dict] = {}
    profiles: dict[tuple[int, int], tuple[int, ...]] = {}
    with open_text(args.trajectories) as handle:
        for line in handle:
            row = json.loads(line)
            key = (row["block"], row["seatRotation"])
            meta = checkpoints.get(key)
            if meta is None:
                continue
            candidate = meta["candidate"]
            profiles.setdefault(key, tuple(row["policyProfiles"][candidate]))
            if row["turn"] == 1:
                setup[key] = row
            if row["turn"] <= args.horizon:
                previous = horizon.get(key)
                if previous is None or row["turn"] > previous["turn"]:
                    horizon[key] = row
            previous = final.get(key)
            if previous is None or row["turn"] > previous["turn"]:
                final[key] = row

    rows: list[dict] = []
    for key, meta in checkpoints.items():
        game = meta["game"]
        candidate = meta["candidate"]
        if game["cutoff"] or game["turns"] <= args.horizon:
            continue
        if key not in setup or key not in horizon or key not in final:
            continue
        s = setup[key]
        h = horizon[key]
        f = final[key]

        def at(record: dict, field: str):
            return record[field][candidate]

        rows.append(
            {
                "key": key,
                "win": int(game["winner"] == candidate),
                "vp": game["points"][candidate],
                "rank": game["ranks"][candidate],
                "turns": game["turns"],
                "profile": PROFILE_NAMES.get(profiles[key], str(profiles[key])),
                "setup_pips": at(s, "productionPips"),
                "pips": at(h, "productionPips"),
                "settlements_extra": max(0, at(h, "settlements") - 2),
                "cities": at(h, "cities"),
                "roads_extra": max(0, at(h, "roadsBuilt") - 2),
                "dev_bought": at(h, "developmentBought"),
                "maritime": at(h, "maritimeTrades"),
                "knights": at(h, "playedKnights"),
                "longest": int(h["longestRoadHolder"] == candidate),
                "army": int(h["largestArmyHolder"] == candidate),
                "final_settlements_extra": max(0, at(f, "settlements") - 2),
                "final_cities": at(f, "cities"),
                "final_roads_extra": max(0, at(f, "roadsBuilt") - 2),
                "final_dev_bought": at(f, "developmentBought"),
                "final_maritime": at(f, "maritimeTrades"),
            }
        )

    if not rows:
        raise SystemExit("No terminal candidate games with the requested pre-outcome horizon")

    winners = [row for row in rows if row["win"]]
    nonwinners = [row for row in rows if not row["win"]]
    print(f"terminal games with pre-outcome horizon: {len(rows)}")
    print(f"candidate wins: {len(winners)} ({len(winners) / len(rows):.3f})")

    print(f"\nWINNERS VS NON-WINNERS AT TURN {args.horizon}")
    if winners and nonwinners:
        for field in [
            "setup_pips", "pips", "settlements_extra", "cities", "roads_extra",
            "dev_bought", "maritime", "knights", "longest", "army",
        ]:
            winner_mean = mean(winners, field)
            nonwinner_mean = mean(nonwinners, field)
            print(
                f"{field:18} win={winner_mean:6.3f} nonwin={nonwinner_mean:6.3f} "
                f"delta={winner_mean - nonwinner_mean:+6.3f}"
            )
    else:
        print("insufficient mixed outcomes in this cohort")

    print("\nOPENING PRODUCTION BUCKETS")
    for lower, upper in [(0, 12), (12, 14), (14, 16), (16, 18), (18, 100)]:
        group = [row for row in rows if lower <= row["setup_pips"] < upper]
        if group:
            print(
                f"pips[{lower},{upper}) n={len(group):3} "
                f"win={sum(row['win'] for row in group) / len(group):.3f} "
                f"vp={mean(group, 'vp'):.2f} rank={mean(group, 'rank'):.2f}"
            )

    print("\nPOLICY PROFILE OUTCOMES")
    by_profile: dict[str, list[dict]] = defaultdict(list)
    for row in rows:
        by_profile[row["profile"]].append(row)
    for profile in PROFILE_NAMES.values():
        group = by_profile[profile]
        if group:
            print(
                f"{profile:16} n={len(group):3} "
                f"win={sum(row['win'] for row in group) / len(group):.3f} "
                f"vp={mean(group, 'vp'):.2f} rank={mean(group, 'rank'):.2f}"
            )

    print(f"\nSIMPLE PLAN CUTS AT TURN {args.horizon}")
    cuts = [
        ("city>=1", lambda row: row["cities"] >= 1),
        ("city=0", lambda row: row["cities"] == 0),
        ("extra_settle>=1 no_city", lambda row: row["settlements_extra"] >= 1 and row["cities"] == 0),
        ("dev>=2 no_city no_settle", lambda row: row["dev_bought"] >= 2 and row["cities"] == 0 and row["settlements_extra"] == 0),
        ("maritime>=3 no_city no_settle", lambda row: row["maritime"] >= 3 and row["cities"] == 0 and row["settlements_extra"] == 0),
        ("city>=1 dev>=1", lambda row: row["cities"] >= 1 and row["dev_bought"] >= 1),
    ]
    for label, predicate in cuts:
        group = [row for row in rows if predicate(row)]
        if group:
            print(
                f"{label:30} n={len(group):3} "
                f"win={sum(row['win'] for row in group) / len(group):.3f} "
                f"setup={mean(group, 'setup_pips'):.2f} pips={mean(group, 'pips'):.2f} "
                f"vp={mean(group, 'vp'):.2f}"
            )

    print("\nWINNER FINAL SHAPES")
    if winners:
        for field in ["final_cities", "final_dev_bought", "final_settlements_extra", "final_roads_extra", "final_maritime"]:
            values = [row[field] for row in winners]
            print(f"{field:24} mean={statistics.mean(values):.2f} median={statistics.median(values):.2f}")
    else:
        print("no candidate winners in this cohort")


if __name__ == "__main__":
    main()
