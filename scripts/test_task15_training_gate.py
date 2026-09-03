#!/usr/bin/env python3
"""Focused regression checks for the Task 15 checkpoint gate."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import types
import unittest


TRAINER_PATH = Path(__file__).with_name("train-strategic-model.py")


def load_trainer():
    # The gate is dependency-free; NumPy is used only by later training paths.
    sys.modules.setdefault("numpy", types.ModuleType("numpy"))
    spec = importlib.util.spec_from_file_location("colonist_strategic_trainer", TRAINER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import {TRAINER_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Task15TrainingGateTest(unittest.TestCase):
    def test_only_the_unchanged_state_and_48_feature_tail_is_canonical(self) -> None:
        trainer = load_trainer()
        is_task15_baseline = getattr(trainer, "is_task15_baseline", lambda *_: False)

        self.assertTrue(is_task15_baseline(0, 48))
        self.assertFalse(is_task15_baseline(1, 48))
        self.assertFalse(is_task15_baseline(0, 47))
        self.assertFalse(is_task15_baseline(0, 0))

    def test_native_policy_teacher_rejects_visit_share_labels(self) -> None:
        trainer = load_trainer()
        record = {
            "players": 4,
            "actor": 0,
            "actions": [{"policy": 0.9}, {"policy": 0.1}],
        }

        self.assertFalse(trainer.has_usable_policy_teacher(record))

        record["actions"] = [{"policy": 1.0}, {"policy": 0.0}]
        self.assertTrue(trainer.has_usable_policy_teacher(record))

    def test_task15_schema_rejects_the_old_action_width(self) -> None:
        trainer = load_trainer()
        is_task15_action_width = getattr(
            trainer, "is_task15_action_width", lambda *_: False
        )

        self.assertTrue(is_task15_action_width(52))
        self.assertFalse(is_task15_action_width(48))

    def test_ablation_cannot_pass_without_the_canonical_baseline(self) -> None:
        trainer = load_trainer()
        feature_ablation_passed = getattr(
            trainer, "task15_feature_ablation_passed", lambda *_: False
        )
        passing_checks = [True, True, True, True, True]

        self.assertFalse(feature_ablation_passed(False, passing_checks))
        self.assertTrue(feature_ablation_passed(True, passing_checks))
        self.assertFalse(
            feature_ablation_passed(True, [True, True, False, True, True])
        )


if __name__ == "__main__":
    unittest.main()
