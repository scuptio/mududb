# Vendored dependencies

This directory holds third-party files vendored into the repository. Each
entry must be recorded here so the tree stays auditable and upgradeable.

## dagre.min.js

- **Package**: [dagre](https://github.com/dagrejs/dagre) — Sugiyama graph
  layout with edge-crossing reduction.
- **Used by**: `script/ci/module_analysis.py`, which embeds the file into the
  self-contained HTML pages of the `cargo make analyze-modules` report
  (crate/module dependency graphs).
- **Version**: *unrecorded* — the vendored bundle does not carry a version
  string, and the version it was taken from was not recorded at vendoring
  time. Identify it against upstream tags on the next touch and update this
  entry.
- **License**: MIT (Copyright (c) 2012-2014 Chris Pettitt). The license header
  is preserved inside `dagre.min.js`; the full text is at
  <https://github.com/dagrejs/dagre/blob/master/LICENSE>.
- **Integrity**: SHA-256 of the vendored file:
  `2cde82baf0b9232c00aa13932945ff92a17fb08c6885a635592330a4c2c567c5`.
- **Upgrade policy**: manual. Download the new release's `dist/dagre.min.js`,
  replace the file, update this entry (version + hash), then re-run
  `cargo make analyze-modules` and eyeball the generated report.
