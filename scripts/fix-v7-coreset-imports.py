from pathlib import Path

path = Path("engine/crates/catan-search/src/shared.rs")
source = path.read_text()
old = "    use colonist_catan_core::{DevCard, GameState, SETTLEMENT_COST};"
new = "    use colonist_catan_core::{CITY_COST, DevCard, GameState, Resource, SETTLEMENT_COST};"
if source.count(old) != 1:
    raise SystemExit(f"expected one shared test import, got {source.count(old)}")
path.write_text(source.replace(old, new))
