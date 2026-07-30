from pathlib import Path

path = Path("engine/crates/catan-search/src/policy.rs")
source = path.read_text()
old = '''    if let Some(end_turn) = end_turn {
        if !truncated
            .iter()
            .any(|(action, _)| matches!(action, Action::EndTurn))
        {
            if truncated.len() == branch_cap {
                truncated.pop();
            }
            truncated.push(end_turn);
        }
    }
'''
new = '''    if let Some(end_turn) = end_turn
        && !truncated
            .iter()
            .any(|(action, _)| matches!(action, Action::EndTurn))
    {
        if truncated.len() == branch_cap {
            truncated.pop();
        }
        truncated.push(end_turn);
    }
'''
if source.count(old) != 1:
    raise SystemExit("EndTurn truncation block not found")
path.write_text(source.replace(old, new))

path = Path("engine/crates/catan-search/src/shared.rs")
source = path.read_text()
old = "    representatives.sort_by(|left, right| left.state.state_hash().cmp(&right.state.state_hash()));"
new = "    representatives.sort_by_key(|particle| particle.state.state_hash());"
if source.count(old) != 1:
    raise SystemExit("representative sort block not found")
path.write_text(source.replace(old, new))
