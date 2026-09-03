# Wave 7 Task 15 GPU learning screen

This directory freezes a superseded Wave 7 GPU screen for audit without treating it as promotion evidence.

## Scope and conclusion

Task 15 adds four road-root-impact action features, taking the action width from 48 to 52. The structural values come from the existing road-impact owner; the learned model does not reimplement road strategy.

The historical run used invalid native-GPU value/regret semantics and is retained only by hash. A corrected policy-only five-fold CUDA screen has now been run at clean commit `ecadf07e0fcf2eca4db0103a0b370d70504b8bdd` against the exact 48-feature baseline. It **rejects** the 52-feature candidate:

- only 2 of 5 grouped folds pass the matched policy-only ablation;
- pooled held-out policy cross-entropy is `2.4273587447` for the 52-feature candidate versus `2.4270200583` for the 48-feature baseline, so the candidate is worse by about `0.00033869`;
- pooled final-choice top-1 agreement is `13.85%` for the candidate versus `16.92%` for the baseline, a `-3.08` percentage-point regression;
- native-GPU records remain policy-only and cannot support strategic value-head/checkpoint evidence;
- no matched candidate-vs-baseline gameplay evaluation exists.

Therefore the four-feature learned tail does not pass the current held-out policy screen, and the learned heads remain optional and unpromoted.

## Frozen input and label contracts

The source consisted of 77 previously frozen takeover snapshots across 24 independent `boardSeed:chanceSeed` groups and both 3-player and 4-player games. Generated corpora are not committed; `provenance.json` retains their counts and SHA-256 hashes.

The corresponding schema-2 native-GPU teacher records used a one-hot policy target on the native engine's final `chosen` action. Adaptive-racing `visits` were diagnostics only; they were not interpreted as PUCT preference mass.

The native teacher run used the RTX 3070 Ti and recorded no GPU deadlines, illegal-action failures, or protocol failures. Its build metadata reports decision revision `9668af0` with `buildDirty=true`; `provenance.json` records that limitation, the PTX hash, and source/output hashes. Wave 7 itself does not modify `engine/crates/catan-wasm/src/native_gpu.rs`.

The raw takeover outcome file was about 8 MB and is intentionally not duplicated here. Its SHA-256 is frozen in `provenance.json`.

## Five-fold CUDA screen

The historical run produced five grouped fold outputs from the same 77-record corpus with deterministic splitting by `boardSeed:chanceSeed` and a matched 48-feature baseline. Across five folds every one of the 24 groups was held out exactly once. Those raw fold files are intentionally not committed: they contain stale machine-readable `accepted=true` fields from the invalidated value/regret interpretation. Their original SHA-256 hashes remain in `provenance.json` for audit, while the summary below carries the explicit invalidation status.

Observed environment:

- device: `cuda`
- GPU: `NVIDIA GeForce RTX 3070 Ti`
- PyTorch: `2.14.0+cu130`
- CUDA runtime reported by PyTorch: `13.0`
- hidden width: `32`
- value epochs: `8`
- policy epochs: `4`
- batch size: `4096`
- policy batch groups: `512`
- seed: `20260728`

Reproduction requires a separately retained teacher corpus whose SHA-256 matches `provenance.json`. With that input, the equivalent fold command shape is:

```bash
uv run --with torch --with numpy \
  python scripts/benchmark-gpu-zoom.py \
  <path-to-hash-matching-schema2-corpus.jsonl> \
  --device cuda \
  --validation-folds 5 \
  --validation-fold-index N \
  --baseline-action-features 48 \
  --output /tmp/wave7-fold-N.json
```

Use `N=0..4`. `--device cuda` is intentional: this evidence package is not authorizing a CPU gameplay or CPU learning benchmark fallback.

`gpu-onehot-5fold-summary.json` contains only the corrected policy-valid fields and explicitly records:

- `evidenceStatus=valid-policy-only-feature-screen`;
- `featureAblationSupported=false`;
- `featureCandidateAccepted=false`;
- `matchedArenaSatisfied=false`;
- `promotionQualified=false`.

## Committed files

- `gpu-onehot-5fold-summary.json` - corrected pooled policy-only screen and per-fold policy metrics.
- `provenance.json` - excluded-corpus, source/build/device hashes, superseded fold-output hashes, and known provenance limitations.
