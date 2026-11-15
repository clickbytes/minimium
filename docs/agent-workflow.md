# Agent workflow and skill routing

This guide routes work in Minimium to **the smallest relevant instruction set**. It does not replace actual Rust source, [architecture ownership](../architecture.toml), [root policy](../AGENTS.md), package READMEs or [ADR 0014](adr/0014-agent-engineering-governance.md).

## Sources of truth

| Question | Read |
| --- | --- |
| What project policy and final-testing gate apply? | [Root AGENTS](../AGENTS.md) |
| What additional rules apply to files being changed? | Nearest nested `AGENTS.md` from the table below |
| Which crate owns an actual dependency or role? | [architecture.toml](../architecture.toml) and real Cargo manifests |
| How does the owner API work **today**? | Current Rust source and the owning crate `README.md` |
| Why was a cross-cutting design selected? | Relevant [ADR](adr/) |
| How should this type of task be executed? | Exactly matching [SKILL.md](../.agents/skills/) (or a small justified combination) |
| What is included and when is it accepted? | Current GitHub Issue, PR diff and recorded evidence |

Root policy is always applicable. Nested policy supplements it and must not relax the global test or file-length gate. Architecture and README describe actual packages; ADRs describe decisions; skills describe a method, not an installed API. Correct stale documentation rather than silently assuming planned architecture is already implemented.

## Scoped instruction tree

| Paths edited | Additional instructions |
| --- | --- |
| Any source | [AGENTS.md](../AGENTS.md) |
| `crates/*` | [crates/AGENTS.md](../crates/AGENTS.md) |
| `crates/client-runtime/*` | [client-runtime/AGENTS.md](../crates/client-runtime/AGENTS.md) |
| `crates/server-runtime/*` | [server-runtime/AGENTS.md](../crates/server-runtime/AGENTS.md) |
| `crates/renderer/*` | [renderer/AGENTS.md](../crates/renderer/AGENTS.md) |
| `apps/client/*` | [apps/client/AGENTS.md](../apps/client/AGENTS.md) |
| `games/sample/*` | [games/sample/AGENTS.md](../games/sample/AGENTS.md) |

An owner-level README is still mandatory before modifying a domain's code. Do not load all 7 AGENTS merely because they exist; read root and the files covering the changed paths.

## Skills: what to load

| Trigger | Skill | Output |
| --- | --- | --- |
| Review SRP, ownership, dependency or duplicate execution | [minimium-boundary-review](../.agents/skills/minimium-boundary-review/SKILL.md) | Evidence-backed findings, positive boundaries and priorities; no code change |
| Migrate existing responsibilities **without** intended behavior changes | [minimium-safe-refactor](../.agents/skills/minimium-safe-refactor/SKILL.md) | Owner before/after, complete migrated callsites and compatibility report |
| Create new module, system, component or reusable mechanism | [minimium-new-system](../.agents/skills/minimium-new-system/SKILL.md) | Owner+phase+typed effects, registered callsites, bounded lifecycle |
| Change world topology, sectors, geospatial state, collision, transfers | [minimium-world-consistency](../.agents/skills/minimium-world-consistency/SKILL.md) | Coherent versions, context, producer/consumer and cancel/commit semantics |
| Change GPU, WGSL, clouds, shadows, sky, HDR or scene uploads | [minimium-rendering](../.agents/skills/minimium-rendering/SKILL.md) | Bounded render path, ownership, layouts, fallbacks and separate visual plan |
| Change CLI, console, input command, properties or admin API | [minimium-command-surface](../.agents/skills/minimium-command-surface/SKILL.md) | Typed parse/owner execution/permission/result and host parity |
| Verify an implementation, PR diff or repo agent policy | [minimium-verify](../.agents/skills/minimium-verify/SKILL.md) | Static check matrix, exact outcomes and residual risks |

A task can use a **small combination** when required (e.g. world-consistency + safe-refactor + verify). One skill is the primary workflow; load additional ones only for the actually touched boundaries. Read each skill's "When NOT to use" before invoking it. Sample-only preset changes need the sample AGENTS/README, not an invented generic content skill.

## Eight concrete routing scenarios

| Request | Primary skill | Required owner/context | Avoid loading | Deliverable |
| --- | --- | --- | --- | --- |
| Audit Renderer vs ClientApplication responsibility | boundary-review | root + apps/client instructions; renderer/client-runtime READMEs + actual callers | all shader internals unless relevant | finding with evidence and confidence |
| Move session state from Renderer to application | safe-refactor | apps/client + client-runtime + renderer contracts; active callsites | unrelated fluid/sky physics | one owner and complete caller migration |
| Add an authoritative gameplay module | new-system | `gameplay`, simulation, server-runtime, apps/server composition | GUI shader files | registered stage/effects + read-only port and budget |
| Fix sector edits visible to movement but not combat | world-consistency | world/terrain/procgen/sectors/simulation/runtime | console font/CLI parser | versioned geometry publication for all queries |
| Change atmospheric clouds in WGSL | rendering | environment observation + renderer/pass graph + apps/client adapter | server admission and storage internals | coherent HDR/depth/occlusion upload and visual review plan |
| Fix reset of `client.debug.atmosphere` | command-surface | commands + client config/application + desktop/terminal | impact mechanics and world generator | canonical codec, layered reset and typed error |
| Review a changed PR before closeout | verify | changed package READMEs, Cargo graph, diff, relevant scripts | the entire unrelated codebase | true PASS/FAIL/NOT RUN/NOT APPLICABLE matrix |
| Change sample-game planet/portal content | no specialist skill for content-only; new-system if adding behavior | games/sample AGENTS+README, apps composition, world definitions | all rendering internals unless asset ABI changes | reproducibility, schema/seed impact and updated consumer |

## One dependent Issue at a time

1. Before beginning: verify predecessor Issue `CLOSED`, current HEAD, working tree (if checkout available), and whether an old PR already implemented the same change. GitHub connectors cannot see uncommitted user-local work.
2. Identify owner and affected consumers; read root/nested AGENTS, matching README, optional specific ADR and selected SKILL.
3. Propose and implement the smallest **complete** vertical change. A split file or added trait without connected consumers does not satisfy owner migration.
4. Examine diff for duplicated paths, unsafe dependency edges, mismatched contexts/revisions, error handling, feature builds and unchanged protocol/save/seed unless explicitly authorized.
5. Use applicable **static** checks: `cargo fmt --all -- --check`, `cargo check --workspace --locked`, client no-default-features check, `python scripts/check_architecture.py`, `python scripts/check_file_lengths.py`, `python scripts/check_static_refactor.py`. Once E048-15 lands, also use `python scripts/check_agent_contracts.py` for instruction changes.
6. Record actual tool and exit code. If unavailable, say `NOT RUN` and why. No unit/integration/E2E suite, game execution, GPU visual acceptance or benchmark until the user explicitly starts final testing.
7. Review PR; merge only after acceptance, then **read back** the result on `main`, post evidence to the Issue and close it. Never mark a checklist CLOSED just because a PR was created.

Parallel read-only analysis is welcome; dependent production changes sharing a state owner are serialized. If an earlier step is incomplete, reopen and repair it before progressing.

## Updating the governance system

| What changed? | Update |
| --- | --- |
| Global policy for all contributors | Root `AGENTS.md`, and ADR if architectural decision |
| Path-specific restriction | Matching nested `AGENTS.md` |
| Actual Cargo ownership or prohibited edge | `architecture.toml` and package README in the **same change** |
| Public API, lifecycle, error semantics or invariants | Owning package README + source |
| Repeatable execution procedure | One corresponding `SKILL.md` |
| New or removed skill / routing conflict | This catalog + skill structure checker |
| Important new tradeoff or superseded decision | Numbered ADR with rationale |

Do not create an eighth skill for a one-off task. Avoid copying long package descriptions into AGENTS and do not treat static source lint as proof of semantic SRP.

## Runner compatibility and verification limits

`.agents/skills/<name>/SKILL.md` is a repository-local convention supported by compatible coding agents, not a claim that every GitHub/AI runner automatically discovers or invokes skills. When automatic discovery is unavailable, open the matching skill explicitly. These files do not spawn independent subagents or grant repository mutation permissions.

The upcoming `scripts/check_agent_contracts.py` checks required files, basic skill frontmatter, sections and a small set of local references. It **cannot** establish that a mutable state is correctly owned, that a runtime behaves properly or that a GPU render looks right. Those remain code review and (when authorized) runtime testing concerns.
