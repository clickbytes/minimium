# Hand-maintained file length policy

Hand-maintained code and shaders must contain no more than 500 physical lines per file. The limit applies to Rust, WGSL, Python, PowerShell, shell, and the other source formats listed by `scripts/check_file_lengths.py`. A physical line is counted even when the final line has no trailing newline. Split code into cohesive modules when a file reaches the limit; do not evade it with compressed or combined lines, generated-looking handwritten code, or source hidden in data files.

## Run the checker

From the repository root, run:

```sh
python scripts/check_file_lengths.py
```

The checker walks the working tree, so it checks both tracked and untracked source files. It excludes version-control metadata, build output (`target`, `build`, `dist`), dependency/vendor trees, and conventional local cache/runtime artifact directories. Its source suffix list and exclusions are explicit in the script. Extend those lists when the repository adopts another source format or a new generated/local artifact directory.

Failures list each offending path and physical line count. Split the file or record a narrow, justified exception below. Exceptions must identify the exact repository-relative source path and explain why splitting it is impractical. A stale exception (the file is gone or no longer exceeds the limit), malformed entry, or duplicate entry also fails the check.

## Generated files and data assets

Generated outputs are not hand-maintained code and are excluded when they live under the listed build/generated artifact directories or use the generated source filename suffixes in the checker. Keep generated output in an identifiable output location and document any project-specific generated directory in this policy and the checker's exclusion list. Do not label handwritten code as generated to bypass the limit.

Data assets such as maps, fixtures, images, audio, and scenario/data documents are not source files and are outside this line limit. If a text file is executable code or a shader, add its suffix to the checker's source list instead of treating it as data. Large data assets should follow their own format-specific validation and storage rules.

## Approved exceptions

Record each active exception on its own line as an HTML comment containing the key `file-length-exception:`, followed by the repository-relative path, a vertical bar, and a specific reason. Keep this section empty when there are no exceptions. The checker rejects malformed, duplicate, and stale entries.

<!-- file-length-exception: third_party/astronomy-engine/source/c/astronomy.c | Verbatim upstream Astronomy Engine v2.1.19 source, pinned by third_party/astronomy-engine/provenance.json; split would invalidate the audited vendor snapshot. -->
<!-- file-length-exception: third_party/astronomy-engine/source/c/astronomy.h | Verbatim upstream Astronomy Engine v2.1.19 header, pinned by third_party/astronomy-engine/provenance.json; split would invalidate the audited vendor snapshot. -->
