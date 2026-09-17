//! Static template table: every project file `mpm-crate` can materialize.
//!
//! Templates live under `templates/<lang>/` and are embedded with
//! `include_str!`. Each file name ends with `.tmpl` (stripped on output) so
//! toolchains do not mistake them for real project files; the output path
//! itself may contain `{{placeholders}}` (e.g. the C# project file).

use crate::scaffold::Lang;

/// One template file: source content plus its path relative to the language
/// template root.
pub struct TemplateFile {
    /// Path relative to `templates/<lang>/`, including the `.tmpl` suffix.
    pub relative_path: &'static str,
    /// Raw template content; placeholders are rendered at scaffold time.
    pub content: &'static str,
}

macro_rules! template {
    ($lang:literal, $path:literal) => {
        TemplateFile {
            relative_path: $path,
            content: include_str!(concat!("../templates/", $lang, "/", $path)),
        }
    };
}

const RUST_TEMPLATES: &[TemplateFile] = &[
    template!("rust", ".cargo/config.toml.tmpl"),
    template!("rust", ".gitignore.tmpl"),
    template!("rust", "Cargo.toml.tmpl"),
    template!("rust", "Makefile.toml.tmpl"),
    template!("rust", "readme.md.tmpl"),
    template!("rust", "build-cfg/transpiler-cfg.toml.tmpl"),
    template!("rust", "package/package.cfg.json.tmpl"),
    template!("rust", "sql/ddl.sql.tmpl"),
    template!("rust", "sql/init.sql.tmpl"),
    template!("rust", "src/lib.rs.tmpl"),
    template!("rust", "src/rust/mod.rs.tmpl"),
    template!("rust", "src/rust/procedures.rs.tmpl"),
];

const ASSEMBLYSCRIPT_TEMPLATES: &[TemplateFile] = &[
    template!("assemblyscript", ".gitignore.tmpl"),
    template!("assemblyscript", "Makefile.toml.tmpl"),
    template!("assemblyscript", "asconfig.json.tmpl"),
    template!("assemblyscript", "package.json.tmpl"),
    template!("assemblyscript", "readme.md.tmpl"),
    template!("assemblyscript", "assembly/procedures.ts.tmpl"),
    template!("assemblyscript", "build-cfg/abort-adapter.wat.tmpl"),
    template!("assemblyscript", "build-cfg/gentypes-mpack.ts.tmpl"),
    template!("assemblyscript", "build-cfg/transpiler-cfg.toml.tmpl"),
    template!("assemblyscript", "package/package.cfg.json.tmpl"),
    template!("assemblyscript", "scripts/patch-component-exports.py.tmpl"),
    template!("assemblyscript", "sql/ddl.sql.tmpl"),
    template!("assemblyscript", "sql/init.sql.tmpl"),
    template!("assemblyscript", "wit/deps/mududb-api/api.wit.tmpl"),
    template!("assemblyscript", "wit/types.wit.tmpl"),
];

const CSHARP_TEMPLATES: &[TemplateFile] = &[
    template!("csharp", ".gitignore.tmpl"),
    template!("csharp", "Makefile.toml.tmpl"),
    template!("csharp", "nuget.config.tmpl"),
    template!("csharp", "readme.md.tmpl"),
    template!("csharp", "{{pascal_name}}.csproj.tmpl"),
    template!("csharp", "package/package.cfg.json.tmpl"),
    template!("csharp", "sql/ddl.sql.tmpl"),
    template!("csharp", "sql/init.sql.tmpl"),
    template!("csharp", "src/MiniMsgPack.cs.tmpl"),
    template!("csharp", "src/MuduSys.cs.tmpl"),
    template!("csharp", "src/Procedures.cs.tmpl"),
    template!("csharp", "wit/deps/mududb-api/api.wit.tmpl"),
    template!("csharp", "wit/types.wit.tmpl"),
];

const PYTHON_TEMPLATES: &[TemplateFile] = &[
    template!("python", ".gitignore.tmpl"),
    template!("python", "Makefile.toml.tmpl"),
    template!("python", "procedures.py.tmpl"),
    template!("python", "readme.md.tmpl"),
    template!("python", "package/package.cfg.json.tmpl"),
    template!("python", "sql/ddl.sql.tmpl"),
    template!("python", "sql/init.sql.tmpl"),
    template!("python", "wit/deps/api/api.wit.tmpl"),
    template!("python", "wit/types.wit.tmpl"),
];

const C_TEMPLATES: &[TemplateFile] = &[
    template!("c", ".gitignore.tmpl"),
    template!("c", "Makefile.toml.tmpl"),
    template!("c", "readme.md.tmpl"),
    template!("c", "package/package.cfg.json.tmpl"),
    template!("c", "sql/ddl.sql.tmpl"),
    template!("c", "sql/init.sql.tmpl"),
    template!("c", "src/mudu_sys.c.tmpl"),
    template!("c", "src/mudu_sys.h.tmpl"),
    template!("c", "src/mududb/codec/mpack.c.tmpl"),
    template!("c", "src/mududb/codec/mpack.h.tmpl"),
    template!("c", "src/mududb/codec/mpack_alloc.c.tmpl"),
    template!("c", "src/mududb/codec/record_bridge.c.tmpl"),
    template!("c", "src/mududb/codec/record_bridge.h.tmpl"),
    template!("c", "src/mududb/types/UniCommandArgv.h.tmpl"),
    template!("c", "src/mududb/types/UniCommandResult.h.tmpl"),
    template!("c", "src/mududb/types/UniDataType.h.tmpl"),
    template!("c", "src/mududb/types/UniDataValue.h.tmpl"),
    template!("c", "src/mududb/types/UniError.h.tmpl"),
    template!("c", "src/mududb/types/UniFsDirent.h.tmpl"),
    template!("c", "src/mududb/types/UniFsOpenArgv.h.tmpl"),
    template!("c", "src/mududb/types/UniFsStat.h.tmpl"),
    template!("c", "src/mududb/types/UniMessage.h.tmpl"),
    template!("c", "src/mududb/types/UniOid.h.tmpl"),
    template!("c", "src/mududb/types/UniProcedureParam.h.tmpl"),
    template!("c", "src/mududb/types/UniProcedureResult.h.tmpl"),
    template!("c", "src/mududb/types/UniQueryArgv.h.tmpl"),
    template!("c", "src/mududb/types/UniQueryResult.h.tmpl"),
    template!("c", "src/mududb/types/UniRecordType.h.tmpl"),
    template!("c", "src/mududb/types/UniResultSet.h.tmpl"),
    template!("c", "src/mududb/types/UniResultType.h.tmpl"),
    template!("c", "src/mududb/types/UniScalar.h.tmpl"),
    template!("c", "src/mududb/types/UniScalarValue.h.tmpl"),
    template!("c", "src/mududb/types/UniSqlParam.h.tmpl"),
    template!("c", "src/mududb/types/UniSqlStmt.h.tmpl"),
    template!("c", "src/mududb/types/UniSyscall.h.tmpl"),
    template!("c", "src/mududb/types/UniTupleRow.h.tmpl"),
    template!("c", "src/procedures.c.tmpl"),
    template!("c", "wit/deps/mududb-api/api.wit.tmpl"),
    template!("c", "wit/types.wit.tmpl"),
];

const GO_TEMPLATES: &[TemplateFile] = &[
    template!("go", ".gitignore.tmpl"),
    template!("go", "Makefile.toml.tmpl"),
    template!("go", "go.mod.tmpl"),
    template!("go", "mpack.go.tmpl"),
    template!("go", "mudusys.go.tmpl"),
    template!("go", "procedures.go.tmpl"),
    template!("go", "readme.md.tmpl"),
    template!("go", "binding/mududb/api/system/empty.s.tmpl"),
    template!("go", "binding/mududb/api/system/system.wasm.go.tmpl"),
    template!("go", "binding/mududb/api/system/system.wit.go.tmpl"),
    template!(
        "go",
        "binding/mududb/{{kebab_name}}/{{kebab_name}}/empty.s.tmpl"
    ),
    template!(
        "go",
        "binding/mududb/{{kebab_name}}/{{kebab_name}}/{{kebab_name}}.exports.go.tmpl"
    ),
    template!(
        "go",
        "binding/mududb/{{kebab_name}}/{{kebab_name}}/{{kebab_name}}.wasm.go.tmpl"
    ),
    template!(
        "go",
        "binding/mududb/{{kebab_name}}/{{kebab_name}}/{{kebab_name}}.wit.go.tmpl"
    ),
    template!("go", "package/package.cfg.json.tmpl"),
    template!("go", "sql/ddl.sql.tmpl"),
    template!("go", "sql/init.sql.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/abi.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/case.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/CHANGELOG.md.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/docs.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/empty.s.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/error.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/error.wasm.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/future.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/hostlayout.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/LICENSE.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/list.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/list_json.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/option.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/README.md.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/RELEASE.md.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/resource.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/result.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/stream.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/tuple.go.tmpl"),
    template!("go", "vendor/go.bytecodealliance.org/cm/variant.go.tmpl"),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/codec/frame.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/codec/mpack.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniCommandArgv.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniCommandResult.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniDataType.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniDataValue.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniError.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniFsDirent.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniFsOpenArgv.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniFsStat.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniMessage.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniOid.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniProcedureParam.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniProcedureResult.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniQueryArgv.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniQueryResult.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniRecordType.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniResultSet.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniResultType.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniScalar.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniScalarValue.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniSqlParam.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniSqlStmt.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniSyscall.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniTupleRow.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/bridge.go.tmpl"
    ),
    template!(
        "go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/wire.go.tmpl"
    ),
    template!("go", "vendor/modules.txt.tmpl"),
    template!("go", "wit/deps/cli/command.wit.tmpl"),
    template!("go", "wit/deps/cli/environment.wit.tmpl"),
    template!("go", "wit/deps/cli/exit.wit.tmpl"),
    template!("go", "wit/deps/cli/imports.wit.tmpl"),
    template!("go", "wit/deps/cli/run.wit.tmpl"),
    template!("go", "wit/deps/cli/stdio.wit.tmpl"),
    template!("go", "wit/deps/cli/terminal.wit.tmpl"),
    template!("go", "wit/deps/clocks/monotonic-clock.wit.tmpl"),
    template!("go", "wit/deps/clocks/wall-clock.wit.tmpl"),
    template!("go", "wit/deps/clocks/world.wit.tmpl"),
    template!("go", "wit/deps/filesystem/preopens.wit.tmpl"),
    template!("go", "wit/deps/filesystem/types.wit.tmpl"),
    template!("go", "wit/deps/filesystem/world.wit.tmpl"),
    template!("go", "wit/deps/io/error.wit.tmpl"),
    template!("go", "wit/deps/io/poll.wit.tmpl"),
    template!("go", "wit/deps/io/streams.wit.tmpl"),
    template!("go", "wit/deps/io/world.wit.tmpl"),
    template!("go", "wit/deps/mududb-api/api.wit.tmpl"),
    template!("go", "wit/deps/random/insecure-seed.wit.tmpl"),
    template!("go", "wit/deps/random/insecure.wit.tmpl"),
    template!("go", "wit/deps/random/random.wit.tmpl"),
    template!("go", "wit/deps/random/world.wit.tmpl"),
    template!("go", "wit/deps/sockets/instance-network.wit.tmpl"),
    template!("go", "wit/deps/sockets/ip-name-lookup.wit.tmpl"),
    template!("go", "wit/deps/sockets/network.wit.tmpl"),
    template!("go", "wit/deps/sockets/tcp-create-socket.wit.tmpl"),
    template!("go", "wit/deps/sockets/tcp.wit.tmpl"),
    template!("go", "wit/deps/sockets/udp-create-socket.wit.tmpl"),
    template!("go", "wit/deps/sockets/udp.wit.tmpl"),
    template!("go", "wit/deps/sockets/world.wit.tmpl"),
    template!("go", "wit/types.wit.tmpl"),
];

/// Returns every template file materialized for `lang`.
pub fn templates_for(lang: Lang) -> &'static [TemplateFile] {
    match lang {
        Lang::Rust => RUST_TEMPLATES,
        Lang::AssemblyScript => ASSEMBLYSCRIPT_TEMPLATES,
        Lang::Csharp => CSHARP_TEMPLATES,
        Lang::Python => PYTHON_TEMPLATES,
        Lang::C => C_TEMPLATES,
        Lang::Go => GO_TEMPLATES,
    }
}
