#!/usr/bin/env python3
"""Portable wallet MPK pipeline; copied to each exported examples/wallet/build.py."""
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
PROJECT = HERE / 'mpk'
SDK = HERE.parents[1]

def run(*args, cwd=PROJECT, env=None):
    subprocess.run([str(a) for a in args], cwd=cwd, env=env, check=True)

def tool(name):
    value = shutil.which(name)
    if not value:
        raise RuntimeError(f'Missing {name}; install the prerequisite described in README.md and add it to PATH')
    return value

def main(lang):
    if lang == 'cs' and (sys.platform != 'linux' or __import__('platform').machine() not in ('x86_64', 'AMD64')):
        raise RuntimeError('C# MPK currently requires Linux x64 and .NET 10; adapter mode supports native platforms')
    for name in ('mtp', 'mgen', 'mpm-build', 'wasm-tools'):
        tool(name)
    for folder in ('generated', 'build', 'package'):
        (PROJECT / folder).mkdir(exist_ok=True)
    output = HERE / 'target'; output.mkdir(exist_ok=True)
    module = f'wallet_{lang}'
    wasm = PROJECT / 'build' / f'{module}.wasm'
    if lang == 'rs':
        env = os.environ.copy()
        env['CARGO_TARGET_DIR'] = str(PROJECT/'build/rust')
        run('cargo', 'build', '--release', '--target', 'wasm32-wasip2', env=env)
        shutil.copyfile(PROJECT/'build/rust/wasm32-wasip2/release/wallet_rs.wasm', wasm)
        package(lang, wasm, output)
        return
    source, generated, language = {
        'as': ('assembly/procedures.ts', 'generated/procedures.gen.ts', 'assembly-script'),
        'py': ('procedures.py', 'generated/procedures_gen.py', 'python'),
        'go': ('procedures.go', 'main_gen.go', 'go'),
        'c': ('src/procedures.c', 'generated/procedures_gen.c', 'c'),
        'cs': ('src/Procedures.cs', 'src/WalletCsWorldExportsImpl.cs', 'csharp'),
    }[lang]
    run('mgen', 'check-sql', '--lang', {'as':'as','py':'python','cs':'cs'}.get(lang,lang),
        '--ddl', 'sql/ddl.sql', '--input', source)
    run('mtp', '--input', source, '--output', generated, '--module', module,
        '--package-desc', 'package/package.desc.json', language)
    world = 'procedures.gen.wit' if lang == 'as' else 'procedures_gen.wit' if lang == 'py' else 'wallet-cs.wit' if lang == 'cs' else 'world.wit'
    shutil.copyfile((PROJECT / generated).with_suffix('.wit'), PROJECT / 'wit' / world)
    if lang == 'py':
        run(tool('componentize-py'), '-d', 'wit', '-w', 'wallet-py', 'componentize', 'procedures_gen',
            '-p', 'generated', '-p', '.', '-p', SDK, '-o', wasm)
    elif lang == 'go':
        run(tool('tinygo'), 'build', '-target=wasip2', '-wit-package', PROJECT/'wit', '-wit-world', 'wallet-go', '-o', wasm, '.')
    elif lang == 'cs':
        run(tool('dotnet'), 'build', 'WalletCs.csproj', '-c', 'Release')
        shutil.copyfile(PROJECT/'bin/Release/net10.0/wasi-wasm/publish/wallet_cs.wasm', wasm)
    elif lang == 'as':
        # Invoke npm through its JS entry point: no .cmd shell wrapper on Windows.
        node = tool('node'); npm = Path(tool('npm')).resolve()
        candidates = [npm, npm.parent/'node_modules/npm/bin/npm-cli.js', npm.parent/'../lib/node_modules/npm/bin/npm-cli.js']
        cli = next((p for p in candidates if p.exists() and p.suffix == '.js'), None)
        if cli is None: raise RuntimeError('Cannot locate npm-cli.js next to npm; install Node/npm')
        run(node, cli, 'ci', '--ignore-scripts', cwd=SDK)
        run(sys.executable, 'scripts/patch-generated-imports.py')
        (PROJECT/'build/as').mkdir(exist_ok=True)
        run(node, SDK/'node_modules/assemblyscript/bin/asc.js', generated, '--outFile', 'build/as/procedures.core.wasm',
            '--optimizeLevel', '3', '--exportRuntime', '--exportStart', '_initialize')
        run(sys.executable, 'scripts/patch-component-exports.py')
        run('wasm-tools', 'component', 'embed', '--world', 'wallet-as', 'wit',
            'build/as/procedures.core.patched.wasm', '-o', 'build/embedded.wasm')
        run('wasm-tools', 'component', 'new', 'build/embedded.wasm', '--adapt', 'env=build-cfg/abort-adapter.wat', '-o', wasm)
    elif lang == 'c':
        wasi = os.environ.get('WASI_SDK')
        if not wasi: raise RuntimeError('Set WASI_SDK to an extracted wasi-sdk (see README.md)')
        wasi = Path(wasi)
        compiler = wasi / 'bin' / ('clang.exe' if os.name == 'nt' else 'clang')
        run(compiler, '--target=wasm32', '-nostdlib', '-fno-builtin', '-O2', '-Wall', '-Wextra', '-Werror', '-std=c99',
            '-isystem', wasi/'share/wasi-sysroot/include/wasm32-wasip1', '-I.', '-Isrc', f'-I{SDK}',
            '-Wl,--no-entry', '-Wl,--export-memory',
            SDK/'mududb/codec/mpack.c', SDK/'mududb/codec/mpack_alloc.c', SDK/'mududb/codec/record_bridge.c',
            'src/mudu_sys.c', 'src/procedures.c', generated, '-o', 'build/core.wasm')
        run('wasm-tools', 'component', 'embed', '--world', 'wallet-c', 'wit', 'build/core.wasm', '-o', 'build/embedded.wasm')
        run('wasm-tools', 'component', 'new', 'build/embedded.wasm', '-o', wasm)
    package(lang, wasm, output)

def package(lang, wasm, output):
    run('wasm-tools', 'validate', wasm)
    result = output / f'wallet-{lang}.mpk'
    run('mpm-build', 'create', '--package-cfg', 'package/package.cfg.json', '--package-desc', 'package/package.desc.json',
        '--ddl-sql', 'sql/ddl.sql', '--initdb-sql', 'sql/init.sql', '--wasm-files', wasm, '--output', result)
    print(result)

if __name__ == '__main__':
    try:
        main(sys.argv[1])
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'Wallet build failed: {error}', file=sys.stderr)
        sys.exit(1)
