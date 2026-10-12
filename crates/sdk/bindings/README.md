# MuduDB language bindings

This directory contains the AssemblyScript, Python, C and Go bindings. C# and Rust SDKs are under [`../mudu_api/`](../mudu_api/).

For supported execution modes, see [Languages and execution modes](../../../doc/dev/sdk_distribution.md#language-and-execution-modes). Python supports componentize-py guests as well as native adapter applications and codec tests.

| Binding | Reference |
|---|---|
| AssemblyScript | [Package and codec tests](assemblyscript/README.md) |
| Python | [Guest API and tests](python/README.md) |
| C | [Types, codecs and memory ownership](c/README.md) |
| Go | [Packages and tests](go/README.md) |

## Generated and handwritten code

The shared types and wire formats come from [`mudu_binding/wit`](../../common/mudu_binding/wit/). Change the WIT or generator and regenerate bindings; do not edit generated files directly. Handwritten facades provide language-specific database, filesystem and adapter entry points. Naming and API conventions are in [Binding API surface](../../../doc/dev/binding_api_surface.md).

## Build distributions

From the repository root:

```bash
cargo make build-sdk
# Build only selected languages:
SDK_LANGS=py,go cargo make build-sdk
```

Builds compile and test the selected SDKs. Install their [prerequisites](../../../doc/dev/sdk_distribution.md#prerequisites) first. For GitHub Releases, follow the [manual publishing workflow](../../../doc/dev/sdk_distribution.md#manual-github-action).
