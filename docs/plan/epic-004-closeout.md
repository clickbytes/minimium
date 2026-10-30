# Epic 004 closeout

On 2026-10-30 the user directed that all completed changes be merged and all
Epic 004 issues be closed. [PR #70](https://github.com/clickbytes/minimium/pull/70)
merged `epic/004-realistic-sky-time` into `main` as
[`33efd93ef3691b7e73f13531db4f549fdc1ebc0f`](https://github.com/clickbytes/minimium/commit/33efd93ef3691b7e73f13531db4f549fdc1ebc0f).
The accepted epic head was `2bd0e3410c59fa05b5922cb4701908b666378e73`.
Both commits have tree `b9cd54874b542f766dc8c1fcd561969554606977`.

The E004-15 [audit](epic-004-audit.md) and static checks passed. The final
scale change also passed workspace check, Clippy with warnings denied, release
build, formatting, architecture and file-length checks, and `git diff --check`.
Windows incremental cache hard-link warnings were unrelated to code lints.

**V01–V20: NOT RUN.** No final game, process, GPU, visual or performance
verification was performed. E004-16 / #68 is closed as not planned for this
closeout at the user's direction; it is not marked PASS. The verified status of
astronomical accuracy, visual output, save migration, live networking and target
performance therefore remains unknown. The [verification procedure](epic-004-final-verification.md)
is retained so those claims can be checked later.

The final code uses protocol v13 and save v9. Supported world dates are
`[2000-01-01T00:00:00Z, 2101-01-01T00:00:00Z)`; a 1,000,000× scale can
exhaust that range quickly. No release was published or production server deployed.
