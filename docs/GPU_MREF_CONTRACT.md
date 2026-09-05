# CPU reference and native GPU Mref contract

## Reference checkpoint

The CPU semantic reference for this migration is commit
`950c1ae9af224f802ad266f268fe7c2f9ff2fe38`. CPU rules, the reference dice
controller, and `deep-maxn-v12` are unchanged by the GPU migration.

This is a pinned implementation checkpoint, not a claim of independent R6
acceptance. The user authorized GPU implementation before that review returns.
A later review can require a forward repair to either backend.

`mref-colonist-linked-2024-v1` remains a named public/reference hypothesis.
CPU/GPU agreement does not establish that the hypothesis is Colonist's current
server algorithm, nor does it establish competitive strength.

## Runtime ownership

The live constructor builds Mref from canonical session dice history and the
engine's canonical player order. Unavailable history or a mapping mismatch
still fails closed. Random/unknown dice modes retain M0 behavior.

For eligible own-turn, non-opening Strategist decisions, the native companion
now accepts both `m0-fair-iid-2d6-v1` and `mref-colonist-linked-2024-v1`.
The existing incoming-trade eligibility exception also remains. Opening and
opponent-turn pondering remain CPU/WASM paths. When native is not installed,
the existing WASM fallback keeps the requested stochastic model. A companion
compatibility error or a post-ready native failure is still reported as an
error; it is not permission to relabel Mref as M0.

The Rust native boundary reconstructs the public posterior with the same CPU
resolver as WASM. Exact mandatory/tactical arbitration and public root proposal
generation remain CPU-owned. CUDA owns the strategic rollout transitions and
statistics. Responses retain the effective stochastic model, belief digest,
public-history diagnostics, and native build identity.

## Controller state on CUDA

`cuda_sim.rs` packs controller state using `ReferenceController` getters.
The resident state grows from 404 to 433 `u32` words. The appended fields are
model identity, posterior-pool offset/count, and 26 controller words:

- eleven deck counts, cards remaining, five recent totals and recent length;
- initialized-player mask and four cumulative seven counts;
- seven-streak owner/count and prepared actor.

Preparation initializes the actor and reshuffles below 13 remaining cards.
Resolution decrements the deck and updates recent totals and seven history.
Descendants carry that state through every subsequent roll. Mref never reuses
a static root probability vector for all future rolls.

CUDA computes the same fixed `2^32` controller probability mass as CPU, using
integer arithmetic and largest-remainder normalization with ascending-total
tie breaks. Fractional long division avoids overflowing `u64` during scaling.
The seven-streak term can be clamped before multiplication once it necessarily
saturates the reference adjustment; the stored streak count is not truncated.

The M0 branch keeps its existing two `rng_range(6)` calls and seed streams.

## Partial public history and sampling

A multi-controller public posterior is uploaded once per distinct full belief.
Equal public beliefs share the immutable pool even across different private
resource worlds. There is no resource-world by dice-world Cartesian expansion.

Each strategic rollout samples one controller according to its fixed public
posterior mass, using a separate deterministic RNG domain. That controller is
then private rollout state. Sampling never controls public root-action
availability. Root expansion leaves the uploaded public posterior unchanged;
a second search or a deeper comparison samples from the same public evidence,
not from a controller accidentally retained from an earlier rollout.

Resident arena lanes materialize their controller when their rollout RNG is
seeded. Direct packed-state comparison requires an exact controller;
`CudaSimPackedState::new` rejects a multi-controller mixture rather than
pretending it is one exact state.

This is latent-state Monte Carlo integration, not GPU execution of CPU Deep
MaxN or its full posterior-conditioning algorithm. Per-controller laws and
transitions match exactly; weighted root mixture recombination matches the CPU
fixed-point distribution. Finite rollout estimates need not match CPU search
values or chosen actions. CPU posterior re-quantization at later observations
and Monte Carlo sampling must not be described as bit-identical full searches.

## Compatibility and generated artifacts

| Boundary | Version |
| --- | --- |
| Native framing protocol | 6, unchanged |
| Native state/semantic schema | 4, requires Mref-capable companion |
| Internal CUDA packed-state ABI | 3 |
| Search identity | `deep-maxn-v12`, unchanged |
| Mref identity | `mref-colonist-linked-2024-v1`, unchanged |

An extension using schema 4 must reject an old M0-only schema-3 companion before
analysis. The arena native client uses schema 4 too. This intentionally requires
updating the extension and companion together; additive JSON alone cannot
prove that an older kernel implements Mref.

`simulation_contract_kernel` checks ABI revision, state words, action words,
and reduction words before use. `build-cuda-ptx.py` stamps the exact source into
`sim.ptx`; the Rust build rejects a stale stamp. No CUDA or Rust source changes
are hidden behind a previously generated artifact.

## Build and verification

From the repository root:

```sh
python3 scripts/build-cuda-ptx.py
python3 scripts/build-cuda-ptx.py --check
npm run build:companion
npm run check
npm run build
```

Use the matching extension `dist/` and companion
`engine/target/release/colonist-assistant-gpu`. Building them does not replace a
running browser extension or install the host. Reload the extension and refresh
Colonist tabs after installing the matching pair. NVIDIA's first load of a new
PTX can incur driver JIT compilation; hardware-test wall time is not a strategic
decision-latency measurement.

Hardware parity:

```sh
cd engine
cargo test -p colonist-catan-search --features cuda-sim --lib \
  cuda_sim::dice_tests -- --test-threads=1 --nocapture
cargo test -p colonist-catan-search --features cuda-sim --lib \
  cuda_sim::tests -- --test-threads=1
cargo run -p colonist-catan-search --features cuda-sim --bin cuda-sim-parity
```

Real native request/response integration, from the repository root:

```sh
COLONIST_NATIVE_BINARY="$PWD/engine/target/release/colonist-assistant-gpu" \
  npx vitest run tests/native-mref-integration.test.ts \
  tests/native-mref-client.test.ts tests/native-gpu-schema.test.ts \
  tests/dice-mode.test.ts --maxWorkers=1
```

The hardware suite exercises 33 exact-controller lanes across 2/3/4 players,
6,336 full-state transitions, reshuffle boundaries, repeat suppression, seven
accounting, 64-controller posterior sampling across 4,096 lanes, root-search
cancellation/repeatability, and preservation of the M0 RNG stream. The native
integration suite exercises real M0, complete Mref, partial Mref, old-handshake
rejection, and unavailable-history rejection. Its D68 board/resource fixture
uses synthetic dice histories solely to test the boundary; those histories
are not attributed to the historical game.

Independent review, current live model adequacy, browser deployment validation,
and controlled competitive benchmarking remain distinct evidence claims.
