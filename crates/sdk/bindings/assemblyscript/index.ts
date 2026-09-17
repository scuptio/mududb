// Package-root entry for asc's plain file-path resolution
// (`@mududb/mududb` -> <pkg>/index.ts). The implementation lives in
// `assembly/`; asc 0.27 does not honor package.json "exports"/"main".

export * from "./assembly/index";
