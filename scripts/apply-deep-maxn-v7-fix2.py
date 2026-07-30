from pathlib import Path

path = Path("engine/crates/catan-search/src/depth.rs")
source = path.read_text()
old = "        let ranked = truncate_root_preserving_end_turn(ranked, self.branch_cap);"
if source.count(old) != 1:
    raise SystemExit(f"expected one immutable root ranking, got {source.count(old)}")
source = source.replace(old, "        let mut ranked = truncate_root_preserving_end_turn(ranked, self.branch_cap);")
old = "                time_budget_ms,\n                opponent_maximizes: true,"
if source.count(old) != 1:
    raise SystemExit(f"expected one opening time budget shorthand, got {source.count(old)}")
source = source.replace(old, "                time_budget_ms: config.time_budget_ms,\n                opponent_maximizes: true,")
path.write_text(source)
