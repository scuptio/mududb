## Bug Fix: Align WIT async declarations with Wasmtime 46 Component Model Async ABI

After upgrading Wasmtime from 42.0.0 to 46.0.0, loading the wasm32-wasip2 component failed with the following error:

```
the `async` canonical option requires an async function type
```

The generated component already used Component Model async canonical ABI operations, such as:

canon lower ... async
canon lift ... async

However, some corresponding WIT function declarations were still missing the async keyword. In Wasmtime 46, Component Model Async / WASI 0.3 support is enabled by default, and async canonical ABI lifting/lowering requires the related component function types to be declared as async func.

The fix was to update the WIT definitions and explicitly mark the affected functions as async:
```
query: async func(query-in: list<u8>) -> list<u8>;
command: async func(command-in: list<u8>) -> list<u8>;

export mp2-deposit: async func(param: list<u8>) -> list<u8>;
export mp2-transfer: async func(param: list<u8>) -> list<u8>;
```

According to the official Wasmtime documentation, Config::wasm_component_model_async enables the Component Model async ABI for lifting and lowering functions, as well as async-related component types such as stream, future, and error-context.

## Official documentation:

Wasmtime Config::wasm_component_model_async:https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.wasm_component_model_async

Wasmtime bindgen! documentation:https://docs.wasmtime.dev/api/wasmtime/component/macro.bindgen.html

Bytecode Alliance WASI 0.3 announcement, noting that Wasmtime 46 ships WASI 0.3.0 with Component Model Async enabled by default:https://bytecodealliance.org/articles/WASI-0.3

This change makes the WIT declarations consistent with the generated async canonical ABI and restores compatibility with Wasmtime 46.