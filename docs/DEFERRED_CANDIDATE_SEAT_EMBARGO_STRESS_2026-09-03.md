# Deferred Candidate-Seat Embargo Stress Track

**Status:** deferred; non-gating for the current product scope.

**Decision date:** 2026-09-03

## Product-scope decision

The current product/gameplay target assumes ordinary domestic trading is available. The user does not intend to play a mode where the candidate/root seat is selectively prevented from domestic trading.

Accordingly, candidate-seat domestic-trade disablement is no longer a Wave 4 promotion gate. Current Wave 4 promotion should be judged from the reviewed tactical evidence, corrected takeover evidence, and the normal-trades P3/P4 whole-game validation campaign.

Do not tune production strategy to improve this artificial stress mode while it is out of scope.

The existing benchmark support is retained because it is useful for future robustness research and costs nothing in ordinary production behavior.

## Preserved evidence

The existing embargo artifacts are preserved as archival stress evidence, not current release-gating evidence.

Development screening identified a real P4 stress weakness:

- P4 candidate-seat embargo mean VP margin: `-1.258`;
- its block-bootstrap 95% interval was entirely below zero;
- zero truncations.

Frozen validation artifacts currently available under `/home/hamza/wave4-agent-s-wholegame/` include:

### P3 candidate-seat embargo

- 100 matched blocks / 300 games;
- 185 candidate wins (`61.67%`);
- mean candidate VP `8.1800`;
- mean best-opponent VP `7.0267`;
- mean VP margin `+1.1533`;
- zero truncations;
- 39,573 candidate decisions;
- `tradeStressMode = candidate_seat_disabled`.

### P4 candidate-seat embargo

- 100 matched blocks / 400 games;
- 143 candidate wins (`35.75%`);
- mean candidate VP `6.6650`;
- mean best-opponent VP `8.6300`;
- mean VP margin `-1.9650`;
- zero truncations;
- 42,716 candidate decisions;
- `tradeStressMode = candidate_seat_disabled`.

These observations indicate that the stress mode is worth revisiting if it ever becomes product-relevant, but they are not a reason to optimize the current ordinary-trading policy toward a mode the user does not plan to use.

## Existing tooling

Commit `e629beb` exposes benchmark-only candidate-seat trade disablement via `--candidate-seat-no-player-trades` in `gpu-sim-agent-benchmark`.

The flag sets the rotated candidate seat's existing `domestic_trade_disabled` bit while leaving opponents' domestic trading available. It is evidence tooling only and must not leak into ordinary production behavior.

Keep this tooling unless a future cleanup explicitly removes the deferred experiment surface.

## Reopen conditions

Reopen this track only if at least one of the following becomes true:

1. candidate/root-seat domestic trading can actually be disabled in the intended product environment;
2. real gameplay/telemetry shows embargo-like trade denial is common enough to affect strength materially;
3. a future release explicitly chooses embargo robustness as an acceptance criterion;
4. another accepted mechanism change makes economic independence under trade denial a first-order product requirement.

When reopening, predeclare the evaluation criterion and fresh evidence partition before looking at additional results. Do not repeatedly extend the existing development/validation sample until a favorable result appears.

## Future work boundary

A future embargo-hardening mission should:

- reproduce the stress weakness with fresh matched blocks;
- separate port/maritime economic independence from unrelated strategy effects;
- identify the causal root-choice/value mechanism before any production change;
- compare any repair against ordinary-trading strength to prevent trading away the primary product objective;
- use a fresh holdout only after the repair is frozen.

Until this track is explicitly reopened, no production strategy repair, extra embargo benchmark, or Wave 4 promotion blocker should be created from these stress-only results.
