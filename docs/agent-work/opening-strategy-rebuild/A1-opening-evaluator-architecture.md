# Agent A — Mission A1: Opening Evaluator Architecture

## Role

Role: implementation  
Review independence: independent review required  
Wave: 1  
Effort: ~45%

## Repository

Repository: /home/hamza/repo/colonist-assistant  
Workspace: /home/hamza/repo/colonist-opening-architecture  
Can start: now  
Depends on: coordination baseline only

## Read first

- /home/hamza/repo/colonist-opening-architecture/AGENTS.md
- /home/hamza/repo/colonist-opening-architecture/docs/JEV_STRATEGY_RESEARCH_PLAYBOOK.md
- /home/hamza/repo/colonist-opening-architecture/docs/agent-work/opening-strategy-rebuild/README.md

Load: causal-coding, systematic-debugging, mcp-harness-router, persistent-agent-loop.

## Objective

Implement approved items 1–5:

1. rework/remove the generic four-player `own - 0.34 * strongest_rival` terminal objective;
2. preserve explicit opponent self-maximization in snake-draft recursion;
3. introduce causal contested-position denial instead of blanket strongest-rival subtraction;
4. value prospective ports through complete-build conversion economics;
5. make future expansion realization explicitly self-funding-aware.

Correct semantics; do not tune hill6758-specific constants.

## Current evidence

- hill6758: own-value model already prefers the all-five root after the first repair, but generic rival subtraction flips the final static result.
- hill6758 matched stream: historical turn 96, all-five turn 92, brick-port turn 84.
- task9783 proves equal-pip portfolio completeness matters in both trade modes.
- Opponents already greedily maximize their own setup-aware opening value during recursion.
- The brick port matters because 2:1 brick conversion changes whole-build affordability; a tiny standalone port bonus under-represents it.
- Expansion targets must not receive full repair credit when the present economy cannot fund the road-plus-settlement project.

## Ownership

You own:

- opening objective/evaluation semantics required by items 1–5;
- focused opening regression fixtures including hill6758/task9783 where causally necessary;
- exact deterministic helper terms for causal denial, port realization, and self-funding.

Neighboring missions own:

- B1: Jev calibration/evaluation methodology;
- D31 lane: midgame liquidity/dev-card defect;
- runtime GPU/CPU routing: out of scope.

## Success conditions

- Four-player terminal opening value no longer uses a blanket strongest-rival penalty that can overturn a better self-position absent causal denial.
- Opponent actions remain self-optimized during the snake draft.
- Denial value is tied to an opponent opportunity actually removed/worsened by the candidate root.
- Prospective port value reflects improvement in complete-build access/conversion.
- Expansion credit is reduced when the current economy cannot realistically fund road+settlement realization.
- hill6758 historical zero-wool root no longer wins for the demonstrated bad reason; brick-port/all-five comparisons are explainable by explicit semantic terms.
- task9783 continues to pass in both trade modes.
- Existing recorded opening corpus has no unexplained regression attributable to the redesign.
- No Jev/network dependency enters production.

## Testing / validation authority

Test changes authorized. Run focused catan-search opening tests, recorded opening corpus tests, and bounded arena/matched simulations needed to falsify the design. Do not run unrelated repository-wide campaigns unless an in-scope failure requires it.

## Execution lifetime

persistent-agent-loop

## Out of scope

- D31 midgame resource-liquidity implementation.
- GPU routing/runtime changes.
- Jev prompt/threshold fitting beyond exposing deterministic facts B1 needs.
- final whole-project integration.

## Finish report

Return:

1. status: complete / blocked / needs decision;
2. Agent A, Mission A1;
3. branch/worktree and commits;
4. exact semantic model implemented for objective, denial, ports, and expansion realization;
5. tests/simulations actually performed;
6. hill6758/task9783 behavior;
7. deviations;
8. unresolved blockers/integration notes.
