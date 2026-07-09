#!/usr/bin/env python3
"""Patch the AssemblyScript core wasm exports to be component-model compatible.

`asc` emits two sets of exports:
  - `__component_adapter_*` / `__component_post_adapter_*` (lowering/raising helpers)
  - `adapter_*` (raw adapter functions expected by wit-component)

`wasm-tools component embed` expects the raw `adapter-*` exports to use kebab-case
names matching the WIT interface (`mududb:component-shim/procedure-create-user#adapter-create-user`).
This script renames the `adapter_*` exports in the core module to kebab-case so that
`wit-component` can correctly alias them when building the component.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORE_WASM = ROOT / "build" / "as" / "procedures.core.wasm"
PATCHED_WASM = ROOT / "build" / "as" / "procedures.core.patched.wasm"


def patch_wasm() -> None:
    wasm_bytes = bytearray(CORE_WASM.read_bytes())

    # Only rename the literal export-name strings in the name section,
    # avoiding the matching function-name fields.  In the wasm core module
    # produced by `asc`, the adapter exports appear twice: once in the
    # function names (prefixed by "export:generated/procedures.gen/") and
    # once as plain export names.  The plain export names sit right before
    # the export kind byte and index.  We locate them by looking for the
    # NUL-terminated name followed by the kind/index bytes that immediately
    # follow in the export section.
    export_kinds = {
        b"adapter_create_user": (0x00, 0x51),      # func 81
        b"adapter_deposit": (0x00, 0x52),          # func 82
        b"adapter_withdraw": (0x00, 0x53),         # func 83
        b"adapter_transfer_funds": (0x00, 0x54),   # func 84
        b"adapter_balance": (0x00, 0x55),          # func 85
    }

    for old_name, (kind_byte, idx_byte) in export_kinds.items():
        new_name = old_name.replace(b"_", b"-")
        if len(new_name) != len(old_name):
            raise RuntimeError(
                f"Cannot patch {old_name!r}: kebab-case length differs"
            )

        start = 0
        patched_count = 0
        while True:
            idx = wasm_bytes.find(old_name, start)
            if idx == -1:
                break
            start = idx + 1

            # The export name is length-prefixed.  Check that the bytes
            # immediately after the name are the expected export kind and
            # index; this identifies the actual export entry, not a function
            # name in the name section.
            end = idx + len(old_name)
            if (
                end + 2 <= len(wasm_bytes)
                and wasm_bytes[end] == kind_byte
                and wasm_bytes[end + 1] == idx_byte
            ):
                wasm_bytes[idx:end] = new_name
                patched_count += 1

        if patched_count != 1:
            raise RuntimeError(
                f"Expected exactly one export entry for {old_name!r}, "
                f"found {patched_count}"
            )

    PATCHED_WASM.write_bytes(wasm_bytes)


if __name__ == "__main__":
    patch_wasm()
