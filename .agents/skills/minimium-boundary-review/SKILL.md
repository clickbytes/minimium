---
name: minimium-boundary-review
description: Review Rust architecture, responsibility boundaries, mutable state ownership, SRP and Cargo dependency direction in Minimium without changing code. Use for architecture audits and evidence-based code review, not for implementation.
---

# Boundary review

This skill is **read-only**. Root and local `AGENTS.md`, `architecture.toml`, actual code and the owning package README remain sources of truth; consult [ADR 0014](../../../docs/adr/0014-agent-engineering-governance.md) for document roles.

## When to use

- A request for architectural code review, SRP/ownership analysis, dependency audits, duplicate execution paths or a refactor backlog.
- Investigating whether an observed bug follows from a broken state/producer/consumer boundary.
- Comparing real crate ownership to `architecture.toml` without writing code.

## When NOT to use

- A straightforward implementation, code formatting, shader-only tuning or an already specified refactor to execute. Use `minimium-safe-refactor`, `minimium-rendering` or `minimium-verify` as appropriate.
- A request to certify gameplay, GPU performance or network correctness without actual execution evidence.

## Workflow

1. Identify repository, current branch/HEAD and scope. The supplied historical issue or file path may be stale; inspect the live tree. GitHub access alone does not expose uncommitted local work.
2. Read root and applicable nested AGENTS, the owning package README, `architecture.toml`, and only the relevant ADRs. Resolve **current implementation** from code, not from planned packages or old documentation.
3. Enumerate public API, state owner, mutable fields, event/command producers, every relevant consumer and phase, I/O/network/GPU dependencies, lifecycle and error paths. Trace an operation end-to-end rather than counting file names.
4. Check real violations: two authorities for one state; GUI-specific backend rule; unauthorized dependency; duplicated parser/executor; stale content/revision; partial publication without explicit outcome; hidden lifecycle or time assumption; sample content in engine.
5. Verify candidate findings by inspecting all guards, alternative paths and affected features. Ask whether the behavior is actually reachable. An `impl Renderer` split over files or a legitimate adapter doing I/O is **not** by itself an architectural defect.
6. Assign severity P1 (correctness/security/integrity), P2 (significant architectural risk), or P3 (localized improvement). Mark evidence as `confirmed statically`, `conditional`, or `design preference`, with explicit confidence.
7. Propose the smallest fix preserving producer-consumer connections, existing typed API and compatibility. Avoid introducing extra ECS, crates, module registries or global abstractions as a reflex.
8. Present positives and uncovered areas as well as failures. If evidence is incomplete, say what source/action would decide it; do not invent runtime outcomes.

## Finding format

| Field | Required content |
| --- | --- |
| ID and priority | Stable ID, P1/P2/P3 |
| Source | Repo revision, file path, symbol and line if verified |
| Evidence | Actual code/data-flow, exact condition and observed vs inferred behavior |
| Boundary | State owner, unauthorized mutation, duplicate rule or missing contract |
| Impact | Precise consequence and affected consumer |
| Fix | Minimal target owner/API, callers and migration sequence |
| Confidence | Confirmed statically / conditional / design preference; remaining verification |

Do not use severity as a proxy for certainty. If a bug depends on a specific runtime ordering, document the precondition.

## Example

**Input:** `Review whether Renderer and ClientApplication have clean session ownership; do not edit code.`

**Illustrative output, not a finding about current HEAD:**

- `BR-01 · P2 · conditional` — Suppose a renderer method updates a session ticket before `ClientApplication` validates the active `WorldContext`. The session owner is bypassed; cancellation could leave prepared content visible. Evidence must cite the actual caller and guard before elevating to a confirmed bug. Fix: move the transition through the application/owner API and migrate all desktop/terminal callers.
- Positive boundary: A frame encoder that only consumes a validated immutable `RenderFrame` is already appropriately separated from authoritative session mutation.

## Completion and evidence

Deliver repository SHA and coverage, per-finding code references and producer/consumer trace, a prioritized repair order, positive boundaries, and explicit `NOT RUN` for tests/runtime/visual checks. This skill does not edit or close implementation issues.
