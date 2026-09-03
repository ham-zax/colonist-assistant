# Colonist base-game rule fidelity map

**Audit date:** 2026-09-04  
**Evidence snapshot:** `docs/colonist-evidence/v320/`  
**Repository baseline for this document branch:** `8cc6cfc5932674f178d9d39b21f9507ed6be7109`  
**Primary product scope:** Colonist base game, currently 2–4 players.  
**Future scope:** Colonist base-game 5–6 and 7–8 player modes.  
**Out of scope here:** Seafarers, Cities & Knights, Rush-specific mechanics, scenarios, and other expansion rules except where they prove that a supposedly universal client path is mode-specific.

This document is the durable compatibility ledger between Colonist's current public client/base-game rules and our engine. It intentionally separates deterministic rules, search/evaluator approximations, browser observability, and future-player-count architecture.

## Evidence hierarchy

Use the strongest available evidence in this order:

1. observed live protocol behavior;
2. observed live runtime state;
3. hashed current shipped client assets;
4. Colonist's published base-game rules;
5. official CATAN base-game rules/FAQ;
6. local engine implementation;
7. inference.

The v320 preserved session has **no live gameplay protocol capture**. Static client and published rules therefore carry much of this audit. Do not relabel static schemas as live observations.

## Executive status

### Strongly aligned for current base 2–4P

- setup settlement/road alternation and snake-order engine logic;
- road #2 bound to settlement #2;
- second-settlement starting-resource grants;
- settlement/city production multipliers;
- robber suppression in the deterministic rules transition;
- maritime 4:1 / generic 3:1 / specific 2:1 ratios;
- best applicable maritime ratio rather than stacking ports;
- maritime trading remaining separate from domestic-player-trade policy;
- `disablePlayerTrades` being our assistant/root-seat policy, not a Colonist room toggle;
- live board/resource/port geometry being read from the instantiated Colonist board rather than regenerated locally.

### Confirmed current defects / gaps

1. **Bank-shortage production:** CPU and CUDA currently give nobody a resource whenever aggregate demand exceeds the bank. Base CATAN rules require a special single-recipient branch: if only one player is owed that resource, that player receives all remaining cards of that type.
2. **Dice-mode observability:** Colonist's client carries `diceSetting`; our `BoardSnapshot`/bridge does not. Search and opening economics therefore always assume fair i.i.d. 2d6.
3. **Synthetic board distribution:** `Board::standard()` freely shuffles all number tokens. Colonist distinguishes ordinary `Classic4P` from `Classic4PRandom` (“Base Random” / no fixed numbers). Live recommendations are unaffected because they ingest the observed board, but synthetic benchmark/training boards are not a faithful sample of ordinary Colonist Classic4P setup.
4. **Player-count architecture:** state, WASM, search value vectors, embargo encoding, CUDA layouts, feature normalization, bank/development-card constants, and arena validation are intentionally 4-player-shaped. Future 5–8P support is a contract migration, not a constant bump.

### Important approximations, not rules bugs

- `production_pips()` in the evaluator gives a currently robbed hex residual value (`0.12`) to model future robber movement even though `GameState::produce()` blocks it completely now.
- opening/evaluation production uses fixed fair-2d6 pip expectations; this is an approximation under Colonist Balanced Dice.
- the current B3 opening-economics patch intentionally does **not** assign an expected conversion rate to domestic negotiations. Opponents may legally trade with players, but the opening build ETA models deterministic/self-controlled production + hand + maritime conversion only.

---

# Rule-by-rule map

## 1. Initial setup

### 1.1 Two settlements and two roads

**Colonist rule:** Colonist's base-game rules state that players place two settlements and two roads, one road coming from each settlement.

**Engine:** `GameState` alternates `Phase::SetupSettlement` and `Phase::SetupRoad { settlement }`.

**Status:** **Exact to base-game contract.**

### 1.2 Snake order

**Base-game contract:** first setup round proceeds in player order; the second proceeds in reverse order.

**Engine owner:** `engine/crates/catan-core/src/state.rs::place_setup_road`:

```text
setup_step < num_players  -> current_player = setup_step
otherwise                 -> current_player = total - setup_step - 1
```

**Colonist-specific evidence:** current client exposes the InitialPlacement settlement/road states and Colonist's rules describe the two placement rounds, but D2 did not passively capture the server's setup-turn succession over live gameplay.

**Status:** **Exact base-game rule; Colonist live-protocol confirmation not archived.** This is safe to encode as a base-game contract.

### 1.3 Road #2 must belong to settlement #2

**Colonist evidence:** published rule says the two opening roads are “one coming from each settlement.” The client beginning-road validator also filters owned settlements through `areAdjacentEdgesEmpty`; after road #1, settlement #1 is no longer an eligible untouched opening anchor.

**Engine owner:** `place_setup_road()` receives the exact `settlement` stored by the immediately preceding `place_setup_settlement()` and rejects edges not touching it.

**Status:** **Exact.**

### 1.4 Starting resources from settlement #2

**Colonist evidence:** published base-game rules say the player receives a resource from each tile surrounding the second settlement.

**Engine owner:** `place_setup_settlement()` grants one card for every adjacent hex with `Some(resource)` during the second placement round. Desert/sea have no `Resource`, so grant zero. Two adjacent hexes of the same resource naturally grant two cards.

**Status:** **Exact for current base game.**

The client protocol's `StartingResources` distribution type is supporting representation evidence; it is not the sole proof of the calculation.

---

## 2. Dice

### 2.1 Random Dice

**Engine:** `GameState::chance_weight()` uses the standard two-dice outcome weights:

```text
2..12 -> 1,2,3,4,5,6,5,4,3,2,1
```

Opening production uses the equivalent `NUMBER_PIPS` representation.

**Status:** **Exact for fair Random Dice.**

### 2.2 Colonist dice mode exists in client state

The current shipped client defines a dice-setting enum including:

```text
Random = 0
Balanced = 1
StressTestSequence = 2
PredefinedDiceSequenceDeleted = 3
```

Room-state data maps the incoming `diceType` into `gameSetting.diceSetting`. The client sends a `ClickedDice` intent and consumes dice state containing `diceThrown`, `dice1`, and `dice2`.

**Important runtime caveat:** the preserved D2 browser session stayed in the lobby. `rootStoreState` and `gameController` were not instantiated, so `rootStoreState.gameSettings.diceSetting` was **not directly evaluated in an active game** in the archived runtime capture.

**Status:** **Shipped-client contract established; active-game runtime capture still desirable.**

### 2.3 Bridge gap

`src/page/bridge.ts` already reads neighboring settings such as:

- `cardDiscardLimit`;
- `victoryPointsToWin`;
- `friendlyRobber`;
- private-game state.

`BoardSnapshot` has no `diceSetting` field and the bridge does not publish it.

**Status:** **Confirmed local observability gap.**

A future contract should distinguish at least:

```text
Random | Balanced | Unknown
```

Do not use `diceSetting ?? 0`; missing/uninitialized state is not proof of Random.

### 2.4 Balanced Dice model

Colonist's first-party Balanced Dice article describes a stateful dice-deck plus recent-roll weighting mechanism. Its prose and linked updated sample code have differed on tuning constants.

**Safe conclusion:** Balanced Dice is stateful/non-i.i.d.; a second static pip table is not an exact stochastic model.

**Unsafe conclusion:** a published reshuffle threshold or weighting constant is proven to be the live v320 production-server contract.

**Current engine:** all chance-search paths ultimately use `GameState::chance_weight()` and opening ETAs use `/36` fair-dice expectation.

**Status:** **Approximate under Balanced Dice.** Mode detection is a separate, safer change from stochastic-model replacement.

---

## 3. Production and robber

### 3.1 Settlement and city multipliers

**Colonist rule:** settlement produces 1 resource; city produces 2.

**Engine:** `building.production_multiplier()` feeds `GameState::produce()`.

**Status:** **Exact.**

### 3.2 Robber blocks the occupied hex

**Colonist rule/client:** the robber blocks future resource production from its tile.

**Rules engine:** `GameState::produce()` skips `self.robber_hex` completely.

**Status:** **Exact deterministic rule.**

**Evaluator distinction:** `catan-search/src/eval.rs::production_pips()` multiplies a currently robbed hex by `0.12`, deliberately valuing the possibility that the robber later moves. Other heuristic terms also use residual robber factors. Do not call the **heuristic valuation** a literal current-roll production model.

### 3.3 Bank shortage — multiple affected players

Official CATAN rule: if a resource type is insufficient to supply all entitled players, none of those players receives that resource on that production roll.

Current CPU engine aggregate-demand branch matches this case.

**Status:** **Exact for multi-recipient shortage.**

### 3.4 Bank shortage — only one affected player

Official CATAN FAQ: if only one player is entitled to that resource and the bank has fewer than required, that player receives all remaining cards of that resource.

Current CPU logic:

```text
if total demand > bank:
    continue  // zero distributed
```

Current CUDA simulator mirrors the aggregate all-or-nothing behavior.

**Status:** **Confirmed CPU + GPU mismatch.**

**Repair boundary:** rules transition plus CUDA simulator/generated parity surfaces. This should be a separate rules-fidelity mission, not folded opportunistically into opening valuation.

**B3 impact:** not a normal opening-economics blocker. Starting-resource bank exhaustion is not a realistic base-4P setup concern, and default opening rollout count is zero.

---

## 4. Maritime trading

### 4.1 Ratios

Current shipped client and local engine agree on:

- default bank ratio 4:1;
- generic port 3:1;
- resource-specific port 2:1.

`GameState::trade_ratios()` starts at 4, applies generic ports with `min(3)`, and sets matching specific-resource ratio to 2.

**Status:** **Exact.**

### 4.2 Multiple ports do not stack multiplicatively

The client maintains one effective `bankTradeRatiosState` value per sold resource; bank-trade construction validates card counts against that ratio. The engine likewise keeps one best ratio per resource.

**Status:** **Exact base-game behavior.**

### 4.3 Maritime trading is separate from domestic trading

In the shipped client, bank trades branch to bank-trade validation rather than domestic player-count/embargo/counteroffer checks. This does **not** mean bank trades have no legality checks: turn/phase, bank inventory, requested resource, amount/ratio and other bank-specific conditions still apply.

Local engine `player_trades_enabled` / `domestic_trade_disabled` gates player negotiation, while maritime actions remain governed by their own legality.

**Status:** **Exact distinction.**

---

## 5. Domestic player trading and our product policy

### 5.1 No Colonist base-room `disablePlayerTrades` toggle found

The current room-state client protocol explicitly maps host/game settings including mode, map, `diceType`, VP target, discard limit, max players, speed, hidden-bank setting and friendly robber. No global domestic-trade enable/disable field is present in that room-setting representation, and the corresponding current client surfaces do not expose such a base multiplayer setting.

**Status:** **Strong shipped-client evidence for current base rooms.**

### 5.2 Our `disablePlayerTrades`

`disablePlayerTrades` exists in Colonist Assistant settings and is converted into per-seat local policy (`domestic_trade_disabled`) so the root/local seat can refuse player trades while other seats continue normal game behavior.

**Status:** **Assistant policy, not Colonist table rule.**

### 5.3 Embargoes

Colonist has directed pairwise embargo state. Our engine represents directed embargoes in a 4×4 bit layout encoded in `u16`.

**Status for 2–4P:** **Semantically aligned.**

**Future 5–8P:** current bit layout is structurally insufficient; see `COLONIST_5_8_PLAYER_SUPPORT.md`.

### 5.4 Opponent domestic trade in opening evaluation

Legal capability and deterministic valuation are different questions.

Under the product policy used in current opening work:

```text
root/local seat: domestic player trades disabled
opponents: normal player trades allowed
all seats: legal maritime trades allowed
```

The current B3 work intentionally models deterministic/self-controlled acquisition only: production, known setup hand and maritime conversion. It does not invent an exchange rate for future opponent negotiations.

**Status:** **Known modeling approximation, not a rules mismatch.** If later evidence supports a calibrated opponent-trade model, add it explicitly rather than silently treating another player's willingness to trade as guaranteed inventory.

---

## 6. Board and map representation

### 6.1 Live board mapping

The shipped client exposes instantiated tile/corner/edge/port state. Our page bridge converts the actual live board into `BoardSnapshot`, and engine opening scarcity is computed from those observed hexes.

**Status:** **Correct architecture for live recommendations.**

### 6.2 Synthetic `Board::standard()` is not normal Colonist Classic4P distribution

The local generator:

- builds the 19-hex base geometry;
- shuffles resource types;
- picks a desert location;
- freely shuffles all 18 number tokens;
- shuffles the 9 standard port types onto fixed coastline positions.

The current Colonist client has separate map identities for `Classic4P` and `Classic4PRandom`. The latter is presented as “Base Random” with random/no fixed numbers.

Therefore unrestricted number-token shuffling should not be described as a faithful generator for ordinary Colonist Classic4P.

**Status:** **Synthetic benchmark-distribution mismatch.**

**Live B3 impact:** none when the real observed board is used.  
**Benchmark/training impact:** results from `Board::standard()` may represent a different opening distribution than ordinary Colonist Classic4P.

A future benchmark-fidelity mission should decide whether to add explicit `Classic4P` versus `Classic4PRandom` board constructors rather than changing the live adapter.

---

## 7. Current opening B2/B3 working-tree observations

This documentation branch intentionally does not contain the uncommitted B2/B3 implementation. The active `main` checkout was observed separately on 2026-09-04 with:

```text
MM engine/crates/catan-search/src/opening.rs
AM engine/crates/catan-search/src/opening_recorded_tests.rs
```

The current full dirty diff relative to `8cc6cfc` includes two conceptually separate repairs.

### 7.1 B2 completion authority

The opening solver now carries `endpoint_complete` alongside values so a cut-off partial setup score cannot become authoritative merely because some other continuation under the root reached a complete snake-draft endpoint.

**Status:** separate implementation lane; not part of this evidence branch.

### 7.2 B3 build economy

The latest dirty version:

- values road/settlement/city/development build access using complete costs rather than independent per-resource proxies;
- includes each player's deterministic setup hand, not only the root's;
- uses `state.trade_ratios(player)` for maritime conversion;
- explicitly does not assign speculative value to domestic negotiations;
- documents the `/36` fair-dice expectation as an approximation when Balanced Dice is active.

An earlier intermediate version discarded opponents' setup hands in the new build-economy term; that issue had already been corrected by the time this document was written. Do not resurrect the stale finding.

**Remaining rule-fidelity dependency:** forwarding dice mode can identify when the fair-i.i.d. assumption is only approximate, but an exact Balanced stochastic replacement still needs separate evidence/modeling.

---

# Engine gap ledger

| ID | Area | Current status | Severity / scope | Authoritative owner(s) | Recommended treatment |
|---|---|---|---|---|---|
| RF-01 | Single-recipient bank shortage | **Mismatch** | Base-game CPU/GPU correctness | `catan-core::produce`, CUDA `produce_roll` + parity contracts | Separate focused rules repair |
| RF-02 | Dice-mode bridge | **Missing observable field** | All live games using Balanced Dice | `BoardSnapshot`, page bridge, worker/WASM contract if consumed | Add explicit Random/Balanced/Unknown state before any stochastic retuning |
| RF-03 | Balanced stochastic search | **Approximation** | Balanced games | chance-state model + search/evaluator consumers | Do not use a static alternate pip table; research/version exact model separately |
| RF-04 | Synthetic Classic4P map | **Distribution mismatch** | Offline benchmarks/training only | `Board::standard` / arena board selection | Introduce explicit map-generation contract if benchmark fidelity requires it |
| RF-05 | Robber production heuristic | **Intentional approximation** | Evaluator, not rules transition | `production_pips` and robber-related evaluator terms | Keep labeled as future-value heuristic; do not confuse with current production rules |
| RF-06 | Opponent domestic trade in opening ETA | **Intentional omission** | Opening strength model | opening evaluator | Only add calibrated trade-opportunity value if evidence justifies it |
| RF-07 | 5–8P player/state widths | **Unsupported** | Future base 5–8P | core/WASM/search/CUDA/arena/protocol contracts | Coordinated migration; see dedicated roadmap |

---

# Safe implementation contracts today

For current base-game 2–4P work, the following may be treated as stable contracts:

1. two setup settlement+road pairs;
2. reverse second placement round;
3. each setup road is anchored to its corresponding setup settlement;
4. second settlement grants one resource per adjacent producing base resource hex;
5. settlement production multiplier 1, city multiplier 2;
6. robber completely suppresses current production on its occupied hex;
7. maritime 4:1 / 3:1 / 2:1 with one best ratio per sold resource;
8. disabling domestic trades for the assistant/root seat must not disable maritime trade;
9. directed embargoes affect domestic player trading;
10. live opening scarcity should be computed from the observed Colonist board;
11. Colonist dice mode exists as a client game-setting concept, and an absent observation must remain Unknown.

# Do not encode as a proven live contract yet

- exact current production-server Balanced Dice tuning constants;
- a fixed alternate `NUMBER_PIPS` table for Balanced Dice;
- a blanket claim that every custom room defaults to Balanced;
- an exact normal-Classic4P server map randomization algorithm based solely on client absence searches;
- a deterministic exchange rate for opponent domestic trading;
- expansion/scenario starting-resource behavior when the product scope is base game.

# External rule references recorded by this audit

- Colonist base-game rules: `https://colonist.io/catan-rules` (checked 2026-09-04).
- Colonist 5–6 / 7–8 rules: `https://colonist.io/catan-rules/5-6-player` and the Colonist rules article covering the Special Build Phase (checked 2026-09-04).
- Colonist Balanced Dice design: `https://blog.colonist.io/balanced-dice-designing/` (checked 2026-09-04; mechanism evidence, not live-server constant proof).
- CATAN base-game FAQ: `https://www.catan.com/faq/basegame` (checked 2026-09-04; bank-shortage branch and other base rules).

For client-specific claims, prefer the hashed local snapshot over a mutable website page.
