# Future Colonist base 5–8 player support

**Current product scope:** 2–4 player base games.  
**Future target:** Colonist's Base Game 5–6P and Base Game 7–8P modes, up to 8 players.  
**Purpose:** record the real migration surface now so current work avoids adding new 4-player assumptions.

This is not an implementation plan to start today. It is the architectural contract map for a future migration.

## Colonist's current extended-base contracts

Colonist's current 5–6 / 7–8 rules page records:

| Contract | 4P base | 5–6P | 7–8P |
|---|---:|---:|---:|
| Board tiles | 19 | 30 | 37 |
| Cards of each resource type | 19 | 24 | 29 |
| Development cards | 25 | 34 | 43 |
| Knights | 14 | 20 | 26 |
| Victory points | 5 | 5 | 5 |
| Year of Plenty | 2 | 3 | 4 |
| Road Building | 2 | 3 | 4 |
| Monopoly | 2 | 3 | 4 |

The extended modes also add a **Special Build Phase** after other players' regular turns. During that phase a player may build roads/settlements/cities or buy a development card, but may not trade, use a development card, or win.

This means future support is not simply `num_players <= 8`. It changes board geometry, bank/deck conservation, turn phases, legality, search branching, and data layouts.

---

# Current 4-player-shaped contracts

## 1. Core board constructor

`engine/crates/catan-core/src/board.rs::Board::standard()`:

- asserts `2..=4` players;
- constructs only the 19-hex radius-2 board;
- assumes the base 4P resource-tile multiset;
- assumes 18 number tokens plus desert;
- places the standard nine port types on that topology.

**Future requirement:** explicit board/map constructors or data-driven map definitions for Classic4P, Classic56P and Classic78P. Do not widen the assertion while still returning 19-hex geometry.

## 2. Bank and development deck

`GameState::new()` currently hardcodes:

```text
bank = [19; 5]
development_deck = [14, 5, 2, 2, 2]
```

Several evaluator/features also normalize against `19` and `25`, and development-card inference uses the 4P composition ratio.

**Future requirement:** a base-mode/ruleset contract must own bank capacity and development-deck composition. State initialization, conservation validation, hidden-card beliefs, features, benchmarks and GPU serialization must read from that contract rather than global 4P constants.

## 3. Discard state

Core and WASM use:

```text
discard_remaining: [u8; 4]
```

**Future requirement:** widen to the supported maximum or move to a player-count-sized representation across core, snapshots and WASM.

## 4. Search value vectors

Exact search, MaxN/depth search, MCTS, learned-value integration, WASM reports, native GPU reports and arena data use `[f32; 4]` per-player values extensively.

Examples include:

- `catan-search/src/eval.rs`;
- `catan-search/src/depth.rs`;
- `catan-search/src/exact.rs`;
- `catan-search/src/mcts.rs`;
- `catan-wasm/src/lib.rs`;
- arena report/snapshot structures.

**Likely minimal bounded migration:** because Colonist's maximum target is known (8) and GPU kernels prefer fixed-width storage, a shared `MAX_BASE_PLAYERS = 8` with an active prefix is likely simpler than unconstrained `Vec` values. This is a design recommendation, not yet an authorized implementation decision.

Any choice must be synchronized across CPU, WASM, GPU and serialized benchmark/replay contracts.

## 5. Domestic trade masks

### Per-seat disable mask

`domestic_trade_disabled: u8` already has enough bits for 8 seats, but validation and all consumers must stop assuming only four active positions.

### Directed embargo matrix

Current core encoding:

```text
bit (A * 4 + B)
domestic_trade_embargoes: u16
```

TypeScript worker code also computes:

```text
1 << (embargoer * 4 + blocked)
```

That is intrinsically a 4×4 matrix.

**Future requirement:** replace the 4×4 encoding with an 8-seat-safe contract (for example, a bounded 8×8 bit matrix) and migrate every producer/consumer together. Do not add compatibility shims that leave two embargo representations permanently active.

## 6. CUDA/GPU state layouts

Current GPU code has explicit `MAX_PLAYERS = 4` / `#define MAX_PLAYERS 4u` in both exact evaluation and simulation. Player-strided state layouts, local arrays, output vectors and batch buffers all depend on it.

**Future requirement:** coordinated CPU/GPU contract migration. Increasing the constant affects:

- serialized state-word offsets;
- state stride and memory footprint;
- output buffer width;
- local/shared array sizes;
- evaluator vectors;
- parity tools;
- PTX/generated artifacts where applicable;
- benchmark capacity and performance assumptions.

This should be treated as one ownership frontier, even if the project later moves the production GPU backend from CUDA to WebGPU.

## 7. Feature normalization and learned models

Current features contain 4P-specific normalizers such as:

```text
num_players / 4
bank_count / 19
development_deck_remaining / 25
```

and public-development inference includes 4P deck-composition assumptions.

**Future requirement:** normalize by ruleset capacities, not literals. Any learned model trained on 2–4P positions must be treated as distribution-shifted for 5–8P until evaluated/retrained; widening tensors alone does not establish model quality.

## 8. Arena and CLI validation

Multiple arena/benchmark entry points reject player counts outside `2..=4` and snapshot schemas contain four-wide arrays.

**Future requirement:** update the replay/benchmark schema together with engine support so 5–8P can be reproduced and compared, not merely played live.

---

# New rule semantics required for 5–8P

## Special Build Phase

This is the biggest semantic difference from the current 4P engine.

Colonist rules currently say the phase occurs after each other player's regular turn. A participating player may:

- build a road;
- build a settlement;
- build a city;
- buy a development card.

They may not:

- make player trades;
- make bank/port trades;
- use a development card;
- win during the Special Build Phase.

A future core implementation therefore needs an explicit phase/actor-order contract. It cannot be accurately represented as an ordinary `Main` turn with a few UI restrictions because:

- winning is suppressed even after reaching the target;
- trading is suppressed;
- dev-card use is suppressed;
- turn ownership remains different from the special builder;
- each eligible other player gets an opportunity in sequence.

Search, rollout, terminal detection, planner turn-distance, action friction, replay snapshots and GPU simulation must all understand the phase.

## Setup order

The existing setup algorithm is mostly player-count dynamic:

```text
N players -> 2N setup settlement/road pairs
first N forward, second N reverse
```

That part is conceptually reusable once board/state widths are migrated. Do not rewrite it as separate 5P/6P/7P/8P tables unless actual Colonist behavior requires a deviation.

## Bank shortage becomes more strategically relevant

Larger player counts and larger production fan-out make shortage cases more frequent. The already-confirmed single-recipient shortage defect should be repaired before claiming 5–8P rules parity.

---

# Browser/extension surfaces

## Mostly dynamic today

Useful existing structures are already list/map based:

- `BoardSnapshot.playerOrder`;
- public player map;
- live hex/vertex/edge arrays;
- bridge extraction of instantiated board state.

These are better positioned for 5–8P than the Rust/GPU value contracts.

## Known hardcoded 4-player bridge/worker assumptions

The worker embargo bit index uses `embargoer * 4 + blocked`. This must migrate with the core representation.

The extension UI and contributor guide currently target/test 3–4 player layouts. Future 5–8P support needs explicit overlay/card-matrix layout work and interaction validation rather than assuming the current compact layout scales.

---

# Recommended future migration order

When 5–8P becomes an active mission, use a coordinated contract migration rather than piecemeal widening:

1. **Define a base-mode/ruleset descriptor** for player maximum, board/map identity, bank counts, development deck and special-build capability.
2. **Migrate player-width contracts** in core state, value vectors, discard state and embargo representation to an 8-seat-safe representation.
3. **Add 30/37-tile board definitions** matching Colonist's observed/official extended maps.
4. **Implement Special Build Phase** in deterministic rules and terminal/win semantics.
5. **Migrate WASM/worker/replay schemas** in one wave.
6. **Migrate GPU layouts/kernels** and re-establish CPU/GPU parity.
7. **Replace 4P feature normalizers** with ruleset-derived capacities; evaluate learned-model compatibility.
8. **Extend arena/benchmarks and extension UI** to 5–8P.
9. **Only then advertise/support live 5–8P recommendations.**

Before source mutation, re-audit the then-current Colonist client because map identifiers, phase protocol and extended-mode behavior may have changed since the v320 snapshot.

---

# What current 4P work should do now

Current B3 and other base-4P work does **not** need to implement any of the above.

It should only avoid making the future migration harder:

- iterate through `state.board.num_players` where the representation is already dynamic;
- do not introduce new literal four-seat loops in opening/evaluator logic;
- do not encode base bank/deck constants in new unrelated locations;
- do not add a second 4×4 embargo representation;
- keep map-generation assumptions separated from live observed-board evaluation;
- keep ruleset-specific behavior behind clear ownership boundaries.

The present 4P engine can remain optimized for four players while its new code avoids unnecessary new coupling to the number four.

## Source recorded

Colonist current extended-base rules checked 2026-09-04: `https://colonist.io/catan-rules/5-6-player`.
