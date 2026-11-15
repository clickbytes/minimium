---
name: minimium-verify
description: Perform Minimium diff review, applicable formatting/compilation/architecture checks and auditable evidence without running deferred test suites. Use before closing code or documentation issues and before a merge; not as a substitute for runtime or visual acceptance.
---

# Static verification and evidence

This skill is a **verification procedure**, not a claim that checks have run. Follow the root `AGENTS.md` gate: unit/integration/E2E suites, running the game, GPU visual sessions and benchmarks are deferred until the user explicitly starts final testing.

## When to use

- Before marking an implementation Issue complete or merging an architectural PR.
- After changing Cargo dependencies, domain ownership, public APIs, source code, WGSL layouts or repo-local agent policies.
- To produce a reproducible static evidence report for an issue or reviewer.

## When NOT to use

- As proof of gameplay, rendering realism, performance, save migration correctness or network runtime behavior.
- To authorize deferred tests on your own or run a command without toolchain access.
- To replace an architecture review tracing actual state owners and consumer paths.

## Workflow

1. Capture repository URL, branch and exact HEAD/PR SHA. Inspect `git status` / `git diff` when a local checkout is accessible; if only a GitHub connector is available, state that uncommitted work cannot be inspected.
2. Enumerate changed files and dependency/feature impact. Find the state owner through `architecture.toml` and README. Check for unintended protocol/save/generator/asset, permission and identity changes.
3. If local Git exists, run `git diff --check`. Inspect public types, error/rollback/partial-result paths, removed dead code and all callers; don't equate a clean diff with behavioral parity.
4. Select **only applicable and available** static commands from the matrix below. Never run test suites or application/benchmark workloads in the ordinary phase.
5. For every command record the exact invocation, working directory/HEAD, exit code or failure to execute, a short relevant diagnostic and one of the four statuses below.
6. Review outcomes. A failed required check blocks completion; an unavailable check becomes a named residual risk and must not be reported as passing. A documentation-only task may be completed with NOT APPLICABLE for Cargo checks if its owner and integration contract are manually reviewed.
7. Provide the final evidence table and manual reviewer checklist. Confirm upstream dependent issues are CLOSED before moving the next issue.

## Static command matrix

| Scope | Command | Notes |
| --- | --- | --- |
| Rust edits | `cargo fmt --all -- --check` | Formatting only, not tests |
| Production Rust/dependencies | `cargo check --workspace --locked` | Compile/type validation, not runtime |
| Terminal/no-GPU client boundary | `cargo check -p minimium-client --no-default-features --locked` | Required when that feature graph can be affected |
| Cargo domain ownership | `python scripts/check_architecture.py` | All-feature metadata plus independent declared feature graphs; needs installed Cargo |
| CPU/hosted exports or Cargo features | `python scripts/check_feature_contracts.py` | Checks declared graphs and compiles CPU renderer, terminal client and embedded-host consumers; never executes them |
| Hand-maintained source/shaders | `python scripts/check_file_lengths.py` | Enforces 500 physical lines / documented exceptions |
| Selected shader exports/layouts | `python scripts/check_static_refactor.py` | Declaration/field comparison, not semantic ownership or GPU ABI proof |
| Agent policy/skills, once added | `python scripts/check_agent_contracts.py` | Structure and local references only |
| WGSL changes | Existing WGSL static validator, **only after inspecting available script/tool invocation** | A successful validation is not a visual pass |
| Optional Rust lints | `cargo clippy --workspace --locked -- -D warnings` if installed/appropriate | Report unsupported environment instead of requiring a new tool |

## Evidence statuses

- `PASS`: tool ran, exit code 0, exact command/HEAD recorded.
- `FAIL`: tool ran and failed, with exit code and diagnostic.
- `NOT RUN`: not executed (e.g. connector-only access or missing Cargo toolchain); reason required.
- `NOT APPLICABLE`: not relevant to the changed files; explain why. Never use this to hide a failed required check.

## Example

**Input:** `Review a PR touching only apps/client/AGENTS.md; do not run tests.`

| Check | Status | Evidence |
| --- | --- | --- |
| Branch file read-back | PASS **only if actually fetched** | Commit SHA + matching file |
| `cargo fmt` | NOT APPLICABLE | No Rust formatting changed |
| `cargo check` | NOT APPLICABLE | Documentation-only diff; no crate/API change |
| Agent contract checker | NOT RUN | If not yet installed or runner unavailable |
| Unit/E2E/GPU benchmarks | NOT RUN | Explicitly deferred by project policy |

Illustration only: do not copy a PASS status without running the operation.

## Completion and evidence

Supply changed files, actual owner/consumers, preserved invariants, exact checks with statuses/exit codes, merge or PR SHA, reviewer observations, compatibility and unverified runtime/test concerns. Keep failures visible; never claim build/visual/gameplay success from source inspection alone.
