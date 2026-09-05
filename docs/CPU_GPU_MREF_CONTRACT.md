# CPU reference and CUDA Mref integration

## Reference baseline

The engineering reference is commit
`950c1ae9af224f802ad266f268fe7c2f9ff2fe38` on
`fix/r6-authority-completion`. It includes the R6 authority repairs through
`07ff6db385928e26cf126c43e7ddfbb423969b18` and the subsequent lifecycle fixes.

The user authorized GPU implementation against this pinned candidate before
independent acceptance. This is a **reference pin**, not a claim that R6 passed.
Independent GPU review subsequently passed the exact range
`950c1ae9af224f802ad266f268fe7c2f9ff2fe38..7023af989d7590df36bb078cb9971fb0ac32859d`.
The integration is now on canonical `main`. Commit `cfcdf1b6a9d3c6558955819be8b7c8b3f6e95849`
adds retained legacy-roll conflict reconciliation and revokes in-flight decisions
when stochastic authority changes. Those ingestion repairs do not change the
pinned engine mathematics; their R6 acceptance remains separate from GPU review.

This integration does not change `catan-core` dice mathematics, CPU search policy,
`deep-maxn-v12`, or the public-history belief construction algorithm. Future
semantic changes must identify a new reference and rerun the affected parity gate.

Identities remain separate:

| Component | Identity |
| --- | --- |
| CPU search policy | `deep-maxn-v12` |
| Native strategic algorithm | `gpu-root-rollout` |
| Legacy stochastic model | `m0-fair-iid-2d6-v1` |
| Balanced-Dice reference hypothesis | `mref-colonist-linked-2024-v1` |
| Public-history posterior policy | `public-history-belief-v1` |
| Native messaging protocol / JSON schema | `6` / `3` |
| Internal CUDA state/reduction ABI | `3` |

Mref is a named public/reference hypothesis, not a reconstruction of Colonist's
hidden server deck or proof of current live-server tuning.

## Ownership and routing

`GameSession.diceHistory` owns public roll evidence. The live constructor and
search adapter bind stochastic actors to the same canonical player ordering.
Unusable Balanced history fails closed; it does not become an M0 request.

Rust resolves the public evidence into one canonical posterior before creating
private resource-world states. `CudaSimPackedState` packs that full posterior
into each resident state. CUDA prepares and conditions it with every simulated
roll. It does not draw every future roll from a static root distribution, sample
a hidden controller into actor-facing authority, or multiply TypeScript resource
worlds by a separate dice-world population.

Eligible own-turn midgame decisions use native CUDA when the companion advertises
the requested stochastic model. Opening and opponent-turn pondering retain their
existing CPU/WASM owners. An unavailable or non-Mref-capable companion leaves an
Mref decision on CPU/WASM with Mref semantics intact.

The native `hello` response adds `stochasticModels`. Missing capabilities mean
M0-only, preserving old protocol-6 companion compatibility without giving those
companions Mref authority. The client checks capabilities before sending an
analysis request and rejects an Mref response with absent or different model
identity. The native parser still rejects unknown models and unusable evidence.

## CUDA posterior contract

The existing game-state fields 0 through 403 are unchanged. Fields 404 and 405
hold the stochastic model and particle count; field 406 starts 64 controller slots
of 28 words each. The total resident layout is 2,198 `u32` words per lane.
M0 has zero-valued additional fields and preserves its previous two-die RNG draws.

Each particle stores its 64-bit mass, remaining total counts, deck size, recent
five totals, initialized-player mask, seven counts, seven-streak owner/count, and
prepared actor. Particle masses sum to `2^32`.

`engine/crates/catan-search/src/cuda/mref.cuh` implements the same rational law
as `engine/crates/catan-core/src/dice.rs`.
CUDA uses 128-bit integer intermediates and largest-remainder normalization.
Outcome ties use ascending total; particle ties use canonical controller order.
Preparation and conditioning coalesce equivalent controllers and preserve the
CPU's floor-before-particle-renormalization behavior. Retired slots are zeroed.
The full public posterior survives descendant state cloning.

`dice_distribution_kernel` exposes probability vectors for verification. Normal
search uses the same distribution and transition functions. The CUDA ABI probe
checks the version, state width, action width, and 12-word reduction contract
before resident buffers are used.

`build-cuda-ptx.py` and the Rust build script hash `sim.cu`, `rollout_cutoff.cuh`,
`mref.cuh`, and the NVRTC options together. Rebuild PTX after editing any of those
sources. Native Mref requires NVRTC device-128-bit support when rebuilding the
artifact; running the packaged companion uses the embedded PTX and driver.

## Verification

Run from the repository root, on a CUDA-capable machine:

```sh
python3 scripts/build-cuda-ptx.py --check
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim --bin cuda-sim-mref-parity
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim --bin cuda-sim-parity
cargo run --release --manifest-path engine/Cargo.toml \
  -p colonist-catan-search --features cuda-sim --bin cuda-sim-generated-parity
npm run build:companion
node scripts/verify-mref-native.mjs
npm run check
npm run build
```

`cuda-sim-mref-parity` compares every packed descendant field and exact dice
vectors across 2-, 3-, and 4-player M0/Mref states, including uncertain posteriors,
reshuffle crossings, 15 VP, Friendly Robber, and discard limit 9. It also executes
strategic root expansion, 96-step rollouts, and reduction.

The native integration gate constructs real adapter requests on the D68 board
fixture with explicitly synthetic dice evidence. It checks protocol capabilities,
M0 and Mref strategic execution, CPU/WASM posterior digest agreement, rejection of
unknown provenance, cancellation, and post-cancellation host responsiveness.
It is not a historical D68 recommendation benchmark.

Observed on the RTX 3070 Ti during implementation:

| Gate | Result |
| --- | --- |
| Mixed M0/Mref full-state transitions | 11,520 matched |
| Exact dice vectors | 3,088 matched |
| Multi-particle lanes in that gate | 12 |
| M0 direct transitions | 16,384 matched |
| M0 GPU-generated transitions | 20,544 matched |
| Native M0 search | 384 rollouts; about 4.05 seconds |
| Native complete-history Mref search | 312 rollouts; about 4.26 seconds |
| Native 64-particle suffix Mref search | 280 rollouts; about 4.03 seconds |

Rollout counts and elapsed time depend on deadline scheduling. Identical final
strategic actions are not the parity criterion: CPU Deep MaxN and GPU rollouts
remain different search policies over the same game-model semantics.

### Startup and packaging

The first uncached driver compilation of the new PTX took about 102 seconds in
this environment. A subsequent native handshake took about 353 ms. Run the
native verification gate as installation/build warmup before entering a live
game; a driver or PTX update can invalidate that cache. Do not confuse this cold
native compilation with the separate cold packaged-WASM smoke requirement.

The built extension is `dist/`; the companion is
`engine/target/release/colonist-assistant-gpu`. Building does not reload Chrome,
replace an installed Windows/WSL launcher, or install native-host registration.
Keep extension and companion artifacts from the same candidate together.

## Claims still requiring separate evidence

This contract and its parity gates do not prove live Balanced-Dice model
adequacy, autonomous target-game readiness, or competitive superiority.
Independent review and live model evaluation remain separate from the user's
decision to proceed with implementation before those reviews return.
