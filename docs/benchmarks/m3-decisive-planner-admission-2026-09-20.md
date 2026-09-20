# M3 decisive planner admission study

Status: complete

Branch: `agent/m1-midgame-transition-study`

Starting commit: `9214f5ee2a3bbf25b83a445613964ee527a4d2d4`

This mission tests whether `decisive_current_turn_plan_replacement` over-admits
ordinary settlement/city progress. It is a research-only study. No production
planner, evaluator, search, WASM, worker, or frontend semantics are being
changed.

## Protocol guardrails

- Candidate admission is based only on deterministic discovery and root
  diagnostics. Forced-root terminal outcomes will not be inspected before the
  candidate manifest is frozen.
- `task9783` is a positive closeout control only and cannot count toward the
  two unrelated non-closeout settlement/city cases required to establish a
  general defect.
- Every admitted replacement must retain both authority layers: the ordinary
  backed-up search winner and the final planner-selected root.
- The complete per-particle current-turn plan is audited to identify the first
  action that makes `materially_decisive_transition` true and classify it as a
  terminal win, settlement, city, Longest Road, or Largest Army transition.
- The study will stop before forced-root continuation if fewer than two
  unrelated non-closeout settlement/city replacements survive admission.

## Research harness checkpoint

The research-only `jev-strategy-lab` now records:

- full-state hash, actor hand, actual/public VP, and victory target at a forced
  root;
- `decisivePlanReplacement` together with the ordinary winner and planner root;
- per-belief-particle complete plan sequences and represented/missing mass;
- the first decisive action and trigger, VP/trophy changes, terminal status,
  and final plan-state hash; and
- a `--stop-after-root-diagnostics` mode that stops before applying the forced
  root, preventing terminal-outcome contamination during candidate selection.

Twelve focused harness tests pass at this checkpoint.

## Frozen candidate corpus

The manifest was frozen at
`benchmark-results/jev-lab/m3-decisive-planner/candidate-manifest-v1.json`
before any forced-root terminal outcome was run or inspected. The three fixed
continuation seeds are `9571001`, `9572003`, and `9573007`.

The four-state corpus is:

1. archival `task9783` Longest Road closeout control (reconstruction limited at
   freeze time);
2. mandatory M1 State A, now correctly classified as a Longest Road transition;
3. `ad33bf6fa633dd7a`, a non-closeout settlement-triggered replacement; and
4. `986017bab0aab69d`, a second unrelated non-closeout settlement-triggered
   replacement.

The fresh discovery pass was bounded to four games at seed `9560001`, 12,000
nodes, and zero wall-clock cutoff. It was stopped early during game 1 after the
second qualifying state had already been found. Candidate selection inspected
decision/root diagnostics only, not forced-root terminal outcomes.

### Mandatory M1 State A — `88b38976489011f5`

At the production-relevant 12,000-node budget, the ordinary winner remains
`BuildSettlement(22)` (`0.001508669`, LCB `0.000171506`) and the final planner
root remains `BuildRoad(53)` (`0.000018203`, LCB `0.000001695`). All 24 belief
particles represent the same planner sequence:

`BuildRoad(53) -> MaritimeTrade(Grain, Lumber, 2) -> BuildRoad(21) -> EndTurn`

The first decisive action is the second road, which gains Longest Road and
moves the actor from 3 to 5 VP. State A is therefore trophy-like control
evidence, not a settlement/city-triggered non-closeout replacement.

### Unrelated ordinary-progress candidate — `ad33bf6fa633dd7a`

At 12,000 nodes, the ordinary winner is `EndTurn` (`0.980305`, LCB `0.926983`)
and the planner replaces it with `BuildSettlement(9)` (`0.938865`, LCB
`0.868178`). All 24 belief particles use
`BuildSettlement(9) -> EndTurn`. The settlement itself is the first decisive
action, moving actual VP from 4 to 5 without terminal or trophy acquisition.
This is one qualifying non-closeout candidate.

### Archived trade candidate — `943b9d6c16d84928`

The planner replaces one maritime trade with another, but every particle's
plan next builds a road and gains Longest Road (3 to 5 VP). This is trophy-like
and does not supply the required second non-closeout case.

### Second unrelated ordinary-progress candidate — `986017bab0aab69d`

At 12,000 nodes, the backed-up winner is `BuildRoad(31)` (`0.345163`, LCB
`0.266377`) while the planner replacement is `BuildRoad(61)` (`0.212206`, LCB
`0.136255`). All 24 represented planner sequences are:

`BuildRoad(61) -> BuildSettlement(35) -> EndTurn`

The settlement is the first decisive action, moving the actor from 4 to 5 VP.
The transition is non-terminal and changes neither Longest Road nor Largest
Army. This state was selected over other qualifying decisions in the same
bounded scan because it has the clearest material ordinary-search advantage.

## Deterministic replacement reproduction

Both non-closeout replacements reproduced with root-only deterministic
diagnostics at 12,000 and 48,000 nodes. No M3 forced-root terminal arm was run
or inspected before this reproduction completed.

### Fixed-budget reproduction

| State | Budget | Ordinary backed-up winner | Planner-selected root | Result |
|---|---:|---|---|---|
| `ad33bf6fa633dd7a` | 12k | `EndTurn`, value `0.980305`, LCB `0.926983` | `BuildSettlement(9)`, value `0.938865`, LCB `0.868178` | Replacement reproduced; settlement is first decisive trigger. |
| `ad33bf6fa633dd7a` | 48k | `EndTurn`, value `0.944363`, LCB `0.858317` | `BuildSettlement(9)`, value `0.930582`, LCB `0.851870` | Replacement persists with a narrower ordinary-search gap. |
| `986017bab0aab69d` | 12k | `BuildRoad(31)`, value `0.345163`, LCB `0.266377` | `BuildRoad(61)`, value `0.212206`, LCB `0.136255` | Replacement reproduced; settlement is first decisive trigger. |
| `986017bab0aab69d` | 48k | `BuildSettlement(35)`, value/LCB `0.266667`/`0.223047` | `BuildRoad(61)`, identical value/LCB | Replacement persists, but deeper search closes the backed-up value gap and changes the ordinary winner. |

The `986…` result is explicitly budget-sensitive rather than evidence that
48,000-node non-reproduction is itself a defect. The common owner remains the
planner admission/replacement path: all represented replacement plans first
become decisive on an ordinary settlement, with completion and decisive mass
1.0 and zero response windows.

## Matched continuation

Exactly 18 terminal arms were run: three reconstructable states × two frozen
roots × three frozen continuation seeds. All arms used 12,000 ordinary nodes,
zero wall-clock cutoff, and event-family split roll/development/steal streams.
Every arm reached one non-cutoff terminal record.

### `ad33bf6fa633dd7a`

| Seed | Ordinary `EndTurn` | Planner `BuildSettlement(9)` | Pair direction |
|---:|---|---|---|
| `9571001` | actor won, 10 VP, turn 76 | actor lost, 9 VP, turn 114 | ordinary |
| `9572003` | actor won, 10 VP, turn 104 | actor won, 10 VP, turn 96 | planner on speed |
| `9573007` | actor won, 10 VP, turn 88 | actor won, 10 VP, turn 84 | planner on speed |

The ordinary root won all three actor games; the replacement won two. Saving
converted to a settlement after 8, 4, and 8 rolls respectively. The replacement
built immediately in all three arms. This is directional evidence against the
replacement in one matched seed, but not uniform evidence that immediate
settlement was harmful.

### `986017bab0aab69d`

| Seed | Ordinary `BuildRoad(31)` | Planner `BuildRoad(61)` | Pair direction |
|---:|---|---|---|
| `9571001` | actor lost, 9 VP, turn 87 | actor won, 10 VP, turn 96 | planner |
| `9572003` | actor won, 10 VP, turn 120 | actor lost, 6 VP, turn 95 | ordinary |
| `9573007` | actor won, 10 VP, turn 100 | actor won, 10 VP, turn 84 | planner on speed |

Each root won two of three actor games. Outcomes reverse direction across the
first two seeds. Both roots converted to a same-turn settlement in every arm:
the ordinary root continued through `BuildRoad(51)` to `BuildSettlement(40)`,
while the planner root built `BuildSettlement(45)`. This state therefore tests
two competing expansion paths, not settlement completion versus failure to
convert.

### M1 State A — `88b38976489011f5`

| Seed | Ordinary `BuildSettlement(22)` | Planner `BuildRoad(53)` | Pair direction |
|---:|---|---|---|
| `9571001` | actor lost, 9 VP, turn 88 | actor lost, 5 VP, turn 81 | ordinary on VP |
| `9572003` | actor lost, 9 VP, turn 81 | actor lost, 8 VP, turn 109 | ordinary |
| `9573007` | actor lost, 7 VP, turn 105 | actor lost, 5 VP, turn 111 | ordinary |

The planner continuation executed the complete diagnosed sequence in all three
arms: `BuildRoad(53)`, grain-for-lumber maritime trade, `BuildRoad(21)`, then
`EndTurn`, gaining Longest Road in the current turn. Neither root won an actor
game, and the ordinary settlement ended with higher actor VP in all three
seeds. The trophy conversion is mechanically genuine, but these three terminal
arms do not validate its replacement quality.

## task9783 control fidelity

The archived task9783 material remains a valid historical description of a
three-road Longest Road closeout from 2 to 4 VP and retains its arbitration unit
regression. It is not an observation-safe M3 forced-root state. The archive
lacks the full board, occupied pieces, private development-card state, RNG
state, replay prefix, and state hash; its tracker also reports missing earlier
history. The available state JSON files are unrelated setup-step-6 states.
No hidden cards were synthesized, and task9783 contributed no terminal arms.
Consequently it cannot authorize an M4 narrowing repair in this study.

## Ownership and hypotheses

The common mechanism is mechanically confirmed:

- `catan-search/src/planner.rs::materially_decisive_transition` marks every
  `BuildSettlement` and `BuildCity` action decisive, independently of terminal,
  trophy, race, or closeout context.
- `catan-search/src/depth.rs::decisive_current_turn_plan_replacement_index`
  admits plans with at least `0.999` completion and decisive mass, zero response
  windows, and a sufficiently larger planner value, then can replace the
  ordinary backed-up winner. The replacement is preserved in
  `provenance.decisive_plan_replacement`.

This broad admission behavior occurred in two unrelated non-closeout states at
12,000 nodes. It is therefore not a one-state logging artifact. However, the
stronger claim that the admission rule causes a recurring harmful replacement
is not established:

- `ad33…` supplies one directionally adverse actor-win result, while two pairs
  are joint actor wins where the replacement finishes sooner.
- `986…` reverses direction by seed, and 48,000-node search makes the two roots'
  backed-up values identical while changing the ordinary winner.
- State A confirms that trophy-like plans remain distinguishable, but its
  matched outcomes do not validate replacement quality.
- The known task9783 positive closeout cannot be reconstructed at adequate
  fidelity, so a proposed narrowing rule cannot be shown to preserve that
  legitimate use case.

Supported: ordinary settlement completion can supply all of the current
"decisive" admission mass and override a materially stronger 12,000-node
ordinary root. Falsified or not supported: a generic conclusion that these
overrides are repeatedly harmful, that deeper search preserves both gaps, or
that current evidence safely identifies a production narrowing boundary.

## Decision

**NO GENERAL DECISIVE-PLANNER DEFECT ESTABLISHED**

No production repair and no M4 implementation are authorized. The study finds
a real broad-classification mechanism and one adverse ordinary-progress case,
but it fails the required repeated matched-direction and validated-closeout
guards. Three-seed outcomes remain screening evidence.

## Recommended next experiment

Capture one natural, observation-safe closeout state at decision time with its
exact board, actor-private cards, development-card state, replay prefix, RNG
identity, and state hash. Pair it with a pre-registered urgent settlement-race
hard negative and the two ordinary-progress states here. Then test a candidate
admission invariant diagnostically: ordinary settlement/city completion alone
is insufficient, while terminal, trophy, or independently demonstrated race
urgency remains admissible. Do not implement that invariant until the recovered
closeout and urgent-race controls both survive it.

## Verification

- `git diff --check`
- `cargo test -p colonist-catan-arena --bin jev-strategy-lab` — 12 passed
- release build of `colonist-catan-arena`
- 12k and 48k root-only deterministic replays for both non-closeout states
- 12k and 48k root-only deterministic replays for State A
- exactly 18 terminal files, one `labHeader` and one non-cutoff `gameEnd` each
- each state/root has exactly the frozen seeds `9571001`, `9572003`, `9573007`
- no production strategy, WASM, worker, or frontend file changed
