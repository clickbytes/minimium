# EPIC E048 — Agent engineering governance closeout

Repository: `clickbytes/minimium`. Parent Epic: [#385](https://github.com/clickbytes/minimium/issues/385). Implementation issues: E048-00..15, all confirmed CLOSED before beginning final integration. Scope: governance artifacts only, **not engine behavior**.

## Baseline and verified change boundary

At planning time `main` was `93f254d04e0860516301af757a3ad1a1aeb0ecad` (2026-11-15). After E048-15 it was `53751a29a9bebce6d1018ccff2cf024bbe0c338d`. The GitHub compare API returned 16 commits and **exactly 19 changed paths**: seven `AGENTS.md`, seven `.agents/skills/*/SKILL.md`, ADR 0014, `docs/agent-workflow.md`, a short link in `docs/architecture.md`, `scripts/check_agent_contracts.py`, and a static-only GitHub Actions workflow. No production Rust/WGSL, Cargo manifest, `architecture.toml`, world content, save/protocol or generator file changed. ADR 0013 still belongs to ambient meteors.

`AGENTS.md` preserves the previous no-unit/integration/E2E-until-authorized policy, 500-physical-line code/shader limit and domain ownership constraints. Local AGENTS add scope-specific rules rather than copying global policy. Skills describe procedures, not implemented engine APIs.

## Recorded sequential merges

| Step | Issue | PR | Merge SHA |
| --- | --- | --- | --- |
| E048-00 | [#386](https://github.com/clickbytes/minimium/issues/386) | [#404](https://github.com/clickbytes/minimium/pull/404) | `39023dd079dfab17cd804ddbcf92c307172fe6ce` |
| E048-01 | [#387](https://github.com/clickbytes/minimium/issues/387) | [#405](https://github.com/clickbytes/minimium/pull/405) | `e7a95204db80ce83604d7865a8f25a8163a030b7` |
| E048-02 | [#388](https://github.com/clickbytes/minimium/issues/388) | [#406](https://github.com/clickbytes/minimium/pull/406) | `4e8077b8cd9410ad45e56cf26973896737e7269d` |
| E048-03 | [#389](https://github.com/clickbytes/minimium/issues/389) | [#407](https://github.com/clickbytes/minimium/pull/407) | `4e6c59d54fb0e02d392db2ae6bb0fa348efe9151` |
| E048-04 | [#390](https://github.com/clickbytes/minimium/issues/390) | [#408](https://github.com/clickbytes/minimium/pull/408) | `3dc00c5b539309ee379f09f52ca0cdaddecb9df1` |
| E048-05 | [#391](https://github.com/clickbytes/minimium/issues/391) | [#409](https://github.com/clickbytes/minimium/pull/409) | `025f9ee47cad4a7c1bc54e033c0efc66a2c25584` |
| E048-06 | [#392](https://github.com/clickbytes/minimium/issues/392) | [#410](https://github.com/clickbytes/minimium/pull/410) | `7c904cd13c47f0d3ce87259a08e351743d88dc4e` |
| E048-07 | [#393](https://github.com/clickbytes/minimium/issues/393) | [#411](https://github.com/clickbytes/minimium/pull/411) | `d5e24daba1966db3022f67689b8346c120732596` |
| E048-08 | [#395](https://github.com/clickbytes/minimium/issues/395) | [#412](https://github.com/clickbytes/minimium/pull/412) | `1877b30f5c9f701e980750a210f3ae7c66eba9c8` |
| E048-09 | [#396](https://github.com/clickbytes/minimium/issues/396) | [#413](https://github.com/clickbytes/minimium/pull/413) | `25a79d497cd0d6bfee0cfb3d3a2e96e404b1694f` |
| E048-10 | [#397](https://github.com/clickbytes/minimium/issues/397) | [#414](https://github.com/clickbytes/minimium/pull/414) | `52be291600c2d707646819833f6c70b32cf18775` |
| E048-11 | [#398](https://github.com/clickbytes/minimium/issues/398) | [#415](https://github.com/clickbytes/minimium/pull/415) | `e01446e71ca3f2f1dc9f796089a3d240640f56c7` |
| E048-12 | [#399](https://github.com/clickbytes/minimium/issues/399) | [#416](https://github.com/clickbytes/minimium/pull/416) | `261e327e92038efa556f7a54b6c982348e7f15bb` |
| E048-13 | [#400](https://github.com/clickbytes/minimium/issues/400) | [#417](https://github.com/clickbytes/minimium/pull/417) | `a0ce87b14329d5317cc1476db55bc2a135d49b3e` |
| E048-14 | [#401](https://github.com/clickbytes/minimium/issues/401) | [#418](https://github.com/clickbytes/minimium/pull/418) | `11ed3bcdd1c22260c2702c7a8aae4cc46d63364a` |
| E048-15 | [#402](https://github.com/clickbytes/minimium/issues/402) | [#419](https://github.com/clickbytes/minimium/pull/419) | `53751a29a9bebce6d1018ccff2cf024bbe0c338d` |

The final E048-16 PR and its merge SHA are recorded in [Issue #403](https://github.com/clickbytes/minimium/issues/403) after its actual merge, not predeclared here.

## Manual source-only review: eight routing cases

The cases were inspected against `docs/agent-workflow.md` and the actual scoped files. **This is a review of instructions, not an execution of gameplay scenarios.**

| Scenario | Reviewed routing | Source-only outcome |
| --- | --- | --- |
| Renderer vs ClientApplication SRP audit | boundary-review + app/runtime/renderer owner files | Correct read-only review, no automatic code mutations |
| Move session owner out of Renderer | safe-refactor + client/runtime owners + verify | Before/after and all callers required; no fake parity |
| Create authoritative gameplay module | new-system + gameplay/server runtime and app composition | Stages, bounded effects and sample ownership explicit |
| Reconcile terrain edit and combat collision | world-consistency + world/terrain/simulation/sectors | Geometry version and ranged query consumers explicit |
| Adjust atmosphere/cloud WGSL | rendering + environment/renderer + client adapter | HDR/depth/occlusion and separate visual acceptance explicit |
| Correct property reset | command-surface + commands/client settings | Typed codec/reset and permission result required |
| Review a PR with only instruction changes | verify + owning docs and changed paths | Clear PASS/FAIL/NOT RUN/NOT APPLICABLE semantics |
| Change sample planet/portal definition | sample AGENTS/README, optionally new-system | Game content remains sample-owned; no mandatory unrelated skill |

Each case has one primary workflow, scoped ownership and an expected output. Existing code may still contain historical boundary violations; governance instructions alone do not repair them.

## Static evidence and unverified work

- Real GitHub Actions on E048-15 PR [run 43700284303](https://github.com/clickbytes/minimium/actions/runs/43700284303): `python3 scripts/check_agent_contracts.py` **PASS** (exit 0).
- Real GitHub Actions after merge into main [run 43700376093](https://github.com/clickbytes/minimium/actions/runs/43700376093): **PASS** (exit 0), message: `7 AGENTS files, 7 named Skills, 22 local Markdown links verified`.
- Source/tree read-back, PR merge states and compare from baseline: **PASS** using GitHub connector.
- `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `scripts/check_architecture.py`, `scripts/check_static_refactor.py`: **NOT RUN / not applicable to this governance-only diff**, no local Cargo checkout; no claim of compilation.
- Unit, integration, E2E, game/GPU runtime visual review and benchmarks: **DEFERRED** under root `AGENTS.md`, requiring explicit user authorization for the final testing phase.
- The additional hand-maintained file-length static check introduced in the final governance workflow is verified by the corresponding GitHub Actions PR/main run; consult Issue #403 for the actual run, exit status and any limitations.

## What this does and does not guarantee

The instructions and statically checked schema help a coding agent pick the correct owner, skill and evidence policy. They do **not** enforce the semantic correctness of ownership across all Rust functions or establish gameplay correctness. For an actual architecture violation, require a separate Issue and real producer→consumer code review. Avoid treating a static checker PASS as test parity.

Close E048 only after [Issue #403](https://github.com/clickbytes/minimium/issues/403) contains the real final merge SHA, static results and confirmation of all 17 CLOSED Issues.
