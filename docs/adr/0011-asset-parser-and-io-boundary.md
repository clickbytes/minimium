# ADR 0011: Format parsing is separate from file access

Status: accepted for the current extraction.

`minimium-assets` parses supplied map, mesh and world-definition data into validated world types. `minimium-asset-io` resolves authored paths, performs bounded reads, prepares geography packages and supplies mesh data to the parser. The parser never opens a file. Mesh references remain relative to the map directory and the adapter checks canonical containment before opening them. Prepared geography is passed through the `minimium-procgen` sampling port; procgen does not resolve paths or open the package.

This preserves a single format implementation while allowing applications to choose storage paths and asset lifetime. The asset-I/O adapter owns opened GEBCO package data and its lifetime. Offline import tools live under `tools/geography`; applications keep the repository-root launchers.
