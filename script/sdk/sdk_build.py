#!/usr/bin/env python3
"""Build self-contained SDK distribution repositories; never publish them."""
import argparse
from fnmatch import fnmatchcase
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import xml.etree.ElementTree as ET

from tooling import executable_name, npm_command, run, venv_python, write_text

ROOT = Path(__file__).resolve().parents[2]
CONFIG = json.loads(Path(__file__).with_name('config.json').read_text(encoding='utf-8'))
EXCLUDED = {'.git', 'node_modules', 'target', 'bin', 'obj', 'build',
            'dist', '__pycache__', '.venv'}
CREDENTIAL_NAMES = {'.secrets', '.aws', '.ssh', '.azure', '.kube', '.npmrc',
                    '.pypirc', '.netrc', '_netrc', 'credentials.json',
                    'id_rsa', 'id_dsa', 'id_ecdsa', 'id_ed25519'}
CREDENTIAL_PATTERNS = ('.env', '.env.*', 'service-account*.json', '*.token',
                       '*.pem', '*.key', '*.p12', '*.pfx', '*.jks', '*.keystore')
ENV_TEMPLATES = {'.env.example', '.env.sample', '.env.template'}
PRIVATE_KEY_HEADER = re.compile(
    rb'-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----')


def check_credential_path(relative):
    """Reject credential paths without reading or displaying their contents."""
    for part in relative.parts:
        name = part.lower()
        if name in ENV_TEMPLATES:
            continue
        if name in CREDENTIAL_NAMES or any(fnmatchcase(name, p) for p in CREDENTIAL_PATTERNS):
            raise ValueError(f'Credential file or directory cannot be exported: {relative}')


def check_credential_content(relative, content):
    """Reject private keys even when their filenames look like ordinary source."""
    if PRIVATE_KEY_HEADER.search(content):
        raise ValueError(f'Private key material cannot be exported: {relative}')


def languages(value):
    selected = value.split(',')
    if not selected or any(x not in CONFIG for x in selected) or len(set(selected)) != len(selected):
        raise ValueError('SDK_LANGS must be a nonempty, unique subset of as,py,cs,go,c,rs')
    return selected


def hashes(directory):
    result = {}
    for path in sorted(directory.rglob('*')):
        relative = path.relative_to(directory)
        check_credential_path(relative)
        if path.is_symlink():
            raise ValueError(f'Symlinks are not allowed in SDK distributions: {path}')
        if path.is_file():
            content = path.read_bytes()
            check_credential_content(relative, content)
            result[relative.as_posix()] = hashlib.sha256(content).hexdigest()
    return result


def copy_source(source, dest, patterns):
    """Copy only allowed source roots, excluding build output at every depth."""
    for pattern in patterns:
        for item in sorted(source.glob(pattern)):
            paths = item.rglob('*') if item.is_dir() else [item]
            for path in paths:
                rel = path.relative_to(source)
                if any(p in EXCLUDED or p.endswith('.egg-info') for p in rel.parts):
                    continue
                check_credential_path(rel)
                if path.is_symlink():
                    raise ValueError(f'Source symlink cannot be exported: {path}')
                if path.is_file() and path.suffix not in {'.pyc', '.pyo'}:
                    check_credential_content(rel, path.read_bytes())
                    target = dest / rel
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(path, target)


def version(lang, repo):
    if lang == 'as':
        value = json.loads((repo / 'package.json').read_text(encoding='utf-8'))['version']
    elif lang in ('py', 'rs'):
        name, section = ('pyproject.toml', 'project') if lang == 'py' else ('Cargo.toml', 'package')
        value = tomllib.loads((repo / name).read_text(encoding='utf-8'))[section]['version']
    elif lang == 'cs':
        value = ET.parse(repo / 'Mudu.Api.csproj').findtext('.//Version')
    else:
        value = CONFIG[lang]['version']
    # First release implementation accepts stable versions only, consistently across registries.
    if not isinstance(value, str) or not re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', value):
        raise ValueError(f'{lang}: expected stable X.Y.Z version, got {value!r}')
    return value


def export(lang, repo, commit):
    repo.mkdir(parents=True)
    conf = CONFIG[lang]
    copy_source(ROOT / conf['source'], repo, conf['roots'])
    shutil.copyfile(ROOT / 'LICENSE', repo / 'LICENSE')
    shutil.copytree(ROOT / 'crates/db-kernel/testing/fixtures/golden/v1',
                    repo / 'tests/fixtures/golden/v1', dirs_exist_ok=True)
    v = version(lang, repo)
    runtime_version = tomllib.loads((ROOT/'crates/bindings/mudu_adapter_ffi/Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    metadata = {'adapter_abi': 1, 'adapter_runtime_version': runtime_version, 'generated_by': 'script/sdk/sdk_build.py', 'source_commit': commit,
                'source_repository': 'https://github.com/scuptio/mududb',
                'repository': f'mududb/mududb-{lang}', 'version': v}
    write_text(repo / 'sdk-source.json', json.dumps(metadata, indent=2) + '\n')
    original = repo / 'README.md'
    original.rename(repo / 'API.md')
    install = {
        'as': f'npm install github:mududb/mududb-as#v{v}',
        'py': f'python -m pip install "mududb @ git+https://github.com/mududb/mududb-py.git@v{v}"',
        'go': f'go get github.com/mududb/mududb-go@v{v}',
        'cs': 'Download the .nupkg Release asset and add its directory as a local NuGet source.\n'
              'Alternatively clone this repository and reference Mudu.Api.csproj.',
        'c': 'python check.py\n# Compile mududb/codec/*.c and add this repository to the include path.',
        'rs': 'mududb = { package = "mudu_api_rust", git = "https://github.com/mududb/mududb-rs", '
              f'tag = "v{v}" }}',
    }[lang]
    write_text(repo / 'README.md', f'<!-- Generated by script/sdk/sdk_build.py. Do not edit manually. -->\n'
        f'# mududb-{lang}\n\n{install}\n\n'
        'This repository is a generated distribution. Submit changes and issues to '
        '[the main repository](https://github.com/scuptio/mududb).\n\n'
        'See [API.md](API.md) for the existing API and capabilities. Guest syscall APIs '
        'require a compatible MuduDB host; this is not a general remote database client. '
        'Only explicitly documented mock backends run without that host.\n\n'
        f'Source commit: `{commit}`. Package version: `{v}`. '
        'Golden tests cover MSSP v1; this does not promise compatibility with every server version.\n'
        + ('\nThis Rust package is the standalone guest SDK, not the main-workspace facade.\n' if lang == 'rs' else ''))
    if lang == 'as':
        path = repo / 'package.json'
        data = json.loads(path.read_text(encoding='utf-8'))
        data['repository'] = {'type': 'git', 'url': 'https://github.com/mududb/mududb-as.git'}
        data['scripts']['test'] = 'node run_mp_corpus_test.mjs && node run_syscall_corpus_test.mjs && node run_lenient_decode_test.mjs'
        data['files'] += ['API.md', 'adapter']
        write_text(path, json.dumps(data, indent=2) + '\n')
    samples = {
        'as': ('smoke.ts', '// Generated by script/sdk/sdk_build.py. Do not edit manually.\n'
               'import { FS_O_RDONLY } from "@mududb/mududb";\n'
               'export function smoke(): i32 { return FS_O_RDONLY; }\n'),
        'py': ('smoke.py', '# Generated by script/sdk/sdk_build.py. Do not edit manually.\n'
               'import mududb, mududb.types, mududb.codec, mududb.db\nprint("mududb imports OK")\n'),
        'c': ('smoke.c', '// Generated by script/sdk/sdk_build.py. Do not edit manually.\n'
              '#include "mududb/codec/mpack.h"\nint main(void) { return 0; }\n'),
        'go': ('smoke.go', '// Generated by script/sdk/sdk_build.py. Do not edit manually.\n'
               'package main\nimport ("fmt"; "github.com/mududb/mududb-go/types")\n'
               'func main() { fmt.Println(types.UniOid{}) }\n'),
    }
    if lang in samples:
        filename, content = samples[lang]
        (repo / 'examples').mkdir(exist_ok=True)
        write_text(repo / 'examples' / filename, content)
    from wallet_export import export_wallet
    export_wallet(lang, repo)
    return v


def validate(lang, repo, artifacts, tools):
    """Validate a disposable copy outside the source checkout and package it."""
    env = os.environ.copy()
    env.update(CARGO_BUILD_JOBS='4', RUST_TEST_THREADS='4', CARGO_PROFILE_DEV_DEBUG='1')
    env.pop('CARGO_TARGET_DIR', None)
    env.pop('PYTHONPATH', None)
    env.pop('GOWORK', None)
    env['GOWORK'] = 'off'
    with tempfile.TemporaryDirectory(prefix=f'mududb-consumer-{lang}-') as tmp:
        work = Path(tmp) / 'sdk'
        shutil.copytree(repo, work)
        def call(*args):
            return run(args, work, env)
        if lang == 'as':
            npm = npm_command(tools['node'], tools['npm'])
            call(*npm, 'ci', '--ignore-scripts')
            call(tools['node'], 'node_modules/assemblyscript/bin/asc.js', 'assembly/index.ts', '--target', 'release')
            for name in ('mp_corpus', 'syscall_corpus', 'lenient_decode'):
                call(tools['node'], f'run_{name}_test.mjs')
            call(*npm, 'pack', '--ignore-scripts', '--pack-destination', artifacts)
            consumer = Path(tmp) / 'consumer'
            consumer.mkdir()
            run([*npm, 'install', '--ignore-scripts', next(artifacts.glob('*.tgz')),
                 'assemblyscript@0.27.32'], consumer, env)
            shutil.copyfile(repo / 'examples/smoke.ts', consumer / 'smoke.ts')
            run([tools['node'], 'node_modules/assemblyscript/bin/asc.js', 'smoke.ts', '-o', 'smoke.wasm'], consumer, env)
        elif lang == 'py':
            call(sys.executable, '-m', 'unittest', 'discover', 'tests', '-v')
            for name in ('mp_corpus', 'syscall_corpus', 'lenient_decode'):
                call(sys.executable, f'run_{name}_test.py')
            call(sys.executable, '-m', 'build', '--outdir', artifacts)
            venv = Path(tmp) / 'venv'
            call(sys.executable, '-m', 'venv', venv)
            python = venv_python(venv)
            consumer = Path(tmp) / 'consumer'
            consumer.mkdir()
            for package in [next(artifacts.glob('*.whl')), next(artifacts.glob('*.tar.gz'))]:
                run([python, '-m', 'pip', 'install', '--force-reinstall', package], consumer, env)
                shutil.copyfile(repo / 'examples/smoke.py', consumer / 'smoke.py')
                run([python, 'smoke.py'], consumer, env)
        elif lang == 'cs':
            call(tools['dotnet'], 'test', 'Mudu.Api.Tests/Mudu.Api.Tests.csproj', '-c', 'Release')
            call(tools['dotnet'], 'run', '--project', 'demo/Mudu.Api.Demo.csproj', '-c', 'Release')
            call(tools['dotnet'], 'pack', 'Mudu.Api.csproj', '-c', 'Release', '-o', artifacts)
            consumer = Path(tmp) / 'consumer'
            run([tools['dotnet'], 'new', 'console', '--framework', 'net8.0', '-o', consumer], Path(tmp), env)
            # Declare the dependency directly; the following restore performs the
            # actual package compatibility check with the isolated package cache.
            project = consumer / 'consumer.csproj'
            tree = ET.parse(project)
            group = ET.SubElement(tree.getroot(), 'ItemGroup')
            ET.SubElement(group, 'PackageReference', Include='mududb', Version=version(lang, repo))
            write_text(project, ET.tostring(tree.getroot(), encoding='unicode') + '\n')
            # dotnet restore misclassifies every --source as local when the first
            # one is local (https://github.com/nuget/home/issues/14624), so the
            # consumer declares its sources in a config file instead.
            config = consumer / 'nuget.config'
            write_text(config, '<?xml version="1.0" encoding="utf-8"?>\n<configuration>\n  <packageSources>\n'
                '    <clear />\n'
                f'    <add key="local" value="{artifacts}" />\n'
                '    <add key="nuget" value="https://api.nuget.org/v3/index.json" />\n'
                '  </packageSources>\n</configuration>\n')
            run([tools['dotnet'], 'restore', '--packages', consumer / '.nuget', '--configfile', config], consumer, env)
            write_text(consumer / 'Program.cs', 'System.Console.WriteLine(typeof(mududb.types.UniOid).FullName);\n')
            run([tools['dotnet'], 'run', '--no-restore'], consumer, env)
        elif lang == 'go':
            if (work / 'go.mod').read_text(encoding='utf-8').splitlines()[0] != 'module github.com/mududb/mududb-go':
                raise ValueError('Go module path must match the distribution repository')
            call(tools['go'], 'build', '-buildvcs=false', './...')
            call(tools['go'], 'vet', './...')
            call(tools['go'], 'test', '-buildvcs=false', './...')
            call(tools['go'], 'run', '-buildvcs=false', './examples')
        elif lang == 'c':
            call(sys.executable, 'check.py', '--cc', tools['clang'], '--cxx', tools['clang++'])
            shutil.copyfile(repo / 'examples/smoke.c', work / 'smoke.c')
            call(tools['clang'], '-std=c99', '-I.', 'smoke.c', *sorted((work / 'mududb/codec').glob('*.c')), '-o', executable_name('smoke'))
            call(work / executable_name('smoke'))
        elif lang == 'rs':
            call('cargo', 'test', '--workspace', '--all-features', '--locked')
            call('cargo', 'run', '-p', 'mudu_api_rust_demo', '--locked')
            call('cargo', 'build', '--lib', '--target', 'wasm32-wasip2', '--features', 'wasm-async', '--locked')
            consumer = Path(tmp) / 'consumer'
            (consumer / 'src').mkdir(parents=True)
            write_text(consumer / 'Cargo.toml', '[package]\nname="sdk_consumer"\nversion="0.1.0"\nedition="2024"\n'
                '[dependencies]\nmududb={package="mudu_api_rust",path="../sdk"}\n')
            write_text(consumer / 'src/main.rs', 'fn main() { let _ = mududb::UniOid { h: 0, l: 1 }; }\n')
            run(['cargo', 'check'], consumer, env)


def python_build_available(executable):
    """Check the actual interpreter outside the checkout to avoid module shadowing."""
    try:
        with tempfile.TemporaryDirectory(prefix='sdk-python-check-') as tmp:
            return subprocess.run(
                [str(executable), '-m', 'build', '--version'], cwd=tmp,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                timeout=15,
            ).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def dotnet_runtime_check(executable):
    """Inspect runtimes visible to the exact host used for C# validation."""
    try:
        result = subprocess.run([executable, '--list-runtimes'], cwd=ROOT,
                                capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=15)
    except (OSError, subprocess.TimeoutExpired) as error:
        return False, str(error)
    available = result.returncode == 0 and re.search(
        r'^Microsoft\.NETCore\.App 8\.0\.\d+\s+\[', result.stdout, re.MULTILINE) is not None
    return available, (result.stdout + result.stderr).strip()


def missing_tool_message(missing, selected, requirements, dotnet_details=None):
    """Explain how to repair PATH/dependencies without changing the user's shell."""
    lines = ['Missing SDK build tools: ' + ', '.join(missing), '',
             'Fix the missing prerequisites in the shell running cargo-make:',
             '  cd ' + shlex.quote(str(ROOT))]
    if 'Python build' in missing:
        lines += ['', f'Python build is unavailable to the current interpreter: {sys.executable}']
        interpreter = venv_python(ROOT / '.venv/sdk-build')
        if interpreter.is_file() and python_build_available(interpreter):
            lines += ['The project virtual environment already has build. Activate it:',
                      '  source .venv/sdk-build/bin/activate']
        else:
            lines += ['Install build in the project virtual environment and activate it:']
            if not interpreter.is_file():
                lines += ['  python3 -m venv .venv/sdk-build']
            lines += ['  source .venv/sdk-build/bin/activate',
                      '  python3 -m pip install --upgrade pip build',
                      'If venv is unavailable on Ubuntu: sudo apt-get install python3-venv']
        lines += ['  python3 -m build --version',
                  'Keep this environment activated when running cargo make build-sdk.']
    if 'go' in missing:
        lines += ['', 'Install Go 1.24+ from https://go.dev/doc/install, or add its bin directory to PATH.',
                  'For an existing standard/user-local installation:',
                  '  export PATH="$HOME/.local/go/bin:/usr/local/go/bin:$PATH"',
                  '  go version']
    if 'node' in missing or 'npm' in missing:
        lines += ['', 'Install Node.js 22 with npm: https://nodejs.org/en/download',
                  'If nvm is installed: nvm install 22 && nvm use 22',
                  '  node --version', '  npm --version']
    if 'dotnet' in missing or '.NET 8 runtime' in missing:
        if dotnet_details:
            executable, output = dotnet_details
            lines += ['', f'Current .NET host: {executable}',
                      'Runtimes reported by this host:', output or '  (none)',
                      'C# tests target net8.0 and require Microsoft.NETCore.App 8.0.x.']
        lines += ['', 'Install the .NET 8 SDK (including its runtime), not just the runtime:',
                  '  curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/mududb-dotnet-install.sh',
                  '  bash /tmp/mududb-dotnet-install.sh --channel 8.0 --install-dir "$HOME/.dotnet"',
                  'Use this installation instead of a Snap/system host that lacks .NET 8:',
                  '  export DOTNET_ROOT="$HOME/.dotnet"',
                  '  export PATH="$DOTNET_ROOT:$PATH"',
                  '  hash -r',
                  '  dotnet --list-sdks', '  dotnet --list-runtimes']
    if 'cargo' in missing or 'rustc' in missing:
        lines += ['', 'Install Rust via https://rustup.rs/ and use the repository-pinned toolchain:',
                  '  export PATH="$HOME/.cargo/bin:$PATH"', '  rustup show']
    packages = sorted({'clang' if tool == 'clang++' else tool for tool in missing
                       if tool in {'git', 'clang', 'clang++'}})
    if packages:
        lines += ['', 'On Ubuntu, install the missing system tools:',
                  '  sudo apt-get update', '  sudo apt-get install -y ' + ' '.join(packages)]
    retry = 'cargo make build-sdk'
    if set(selected) != set(CONFIG):
        retry = 'SDK_LANGS=' + ','.join(selected) + ' ' + retry
    lines += ['', 'After fixing all missing tools, retry:', '  ' + retry]
    if not {'git', 'cargo', 'rustc'}.intersection(missing):
        available = [lang for lang in selected
                     if not set(requirements[lang]).intersection(missing)
                     and not (lang == 'py' and 'Python build' in missing)
                     and not (lang == 'cs' and '.NET 8 runtime' in missing)]
        if available:
            lines += ['', 'Or build only the selected languages whose prerequisites are available:',
                      '  SDK_LANGS=' + ','.join(available) + ' cargo make build-sdk']
    lines += ['', 'Installation details: doc/dev/sdk_distribution.md#prerequisites']
    if sys.platform == 'win32':
        # Diagnostics are PowerShell commands; the build itself never invokes a shell.
        replacements = {
            '  cd ' + shlex.quote(str(ROOT)): '  Set-Location -LiteralPath "' + str(ROOT).replace('"', '`"') + '"',
            '  source .venv/sdk-build/bin/activate': r'  .\.venv\sdk-build\Scripts\Activate.ps1',
            '  export DOTNET_ROOT="$HOME/.dotnet"': '  $env:DOTNET_ROOT = "$env:ProgramFiles\\dotnet"',
            '  export PATH="$DOTNET_ROOT:$PATH"': '  $env:PATH = "$env:DOTNET_ROOT;$env:PATH"',
            '  export PATH="$HOME/.cargo/bin:$PATH"': '  $env:PATH = "$env:USERPROFILE\\.cargo\\bin;$env:PATH"',
            '  export PATH="$HOME/.local/go/bin:/usr/local/go/bin:$PATH"': '  $env:PATH = "C:\\Program Files\\Go\\bin;$env:PATH"',
        }
        portable = []
        for line in lines:
            if any(token in line for token in ('sudo apt-get', 'If venv is unavailable', '  hash -r', 'If nvm is installed:')):
                continue
            if 'curl -fsSL https://dot.net/' in line:
                portable.append('Install the Windows .NET 8 SDK: https://dotnet.microsoft.com/download/dotnet/8.0')
                continue
            if 'bash /tmp/mududb-dotnet-install.sh' in line:
                continue
            if line == 'On Ubuntu, install the missing system tools:':
                portable.append('Install Git for Windows and LLVM; use a Visual Studio Developer PowerShell with MSVC Build Tools and the Windows SDK.')
                continue
            line = replacements.get(line, line).replace('python3 -m ', 'python -m ').replace('a Snap/system host', 'another host')
            if line.startswith('  SDK_LANGS='):
                langs = line.split('=', 1)[1].split(' ', 1)[0]
                line = f'  $env:SDK_LANGS = "{langs}"; cargo make build-sdk'
            portable.append(line)
        lines = portable
    elif sys.platform == 'darwin':
        lines = [line for line in lines if 'sudo apt-get' not in line and 'If venv is unavailable' not in line]
        lines = ['Install Xcode Command Line Tools (xcode-select --install) and Git.'
                 if line == 'On Ubuntu, install the missing system tools:' else line for line in lines]
    return '\n'.join(lines)


def preflight(selected):
    requirements = {'as': ['npm', 'node'], 'py': [], 'cs': ['dotnet'],
                    'go': ['go'], 'c': ['clang', 'clang++'], 'rs': ['cargo']}
    tools = {}
    missing = []
    for name in sorted({'cargo', 'git', 'rustc'} | {x for lang in selected for x in requirements[lang]}):
        path = shutil.which(name)
        if not path:
            missing.append(name)
        else:
            tools[name] = path
    if 'py' in selected and not python_build_available(sys.executable):
        missing.append('Python build')
    dotnet_details = None
    if 'cs' in selected and 'dotnet' in tools:
        available, output = dotnet_runtime_check(tools['dotnet'])
        if not available:
            missing.append('.NET 8 runtime')
            dotnet_details = (tools['dotnet'], output)
    if not missing and 'as' in selected:
        npm_command(tools['node'], tools['npm'])
    if missing:
        raise ValueError(missing_tool_message(missing, selected, requirements, dotnet_details))
    return tools


def build(selected, output, with_adapter_runtime=False, with_examples=False, example_mode="both"):
    output.mkdir(parents=True, exist_ok=True)
    # A failed new batch must not leave a previous successful manifest publishable.
    (output / 'manifest.json').unlink(missing_ok=True)
    tools = preflight(selected)
    run([sys.executable, ROOT / 'script/ci/check_bindings_regen.py'], ROOT)
    commit = run(['git', 'rev-parse', 'HEAD'], ROOT, capture=True).strip()
    dirty = bool(run(['git', 'status', '--porcelain', '--untracked-files=normal'], ROOT, capture=True).strip())
    library = None
    if with_adapter_runtime or with_examples:
        from adapter_runtime import build_runtime
        library = build_runtime(output / 'adapter-runtime')
    entries = []
    versions = {name: run([*(npm_command(tools['node'], path) if name == 'npm' else [path]),
                              'version' if name == 'go' else '--version'], ROOT, capture=True).strip()
                for name, path in tools.items()}
    versions['python'] = sys.version
    with tempfile.TemporaryDirectory(prefix='sdk-build-') as tmp:
        staging = Path(tmp)
        for lang in selected:
            name = f'mududb-{lang}'
            package = staging / name
            repo, artifacts = package / 'repo', package / 'artifacts'
            artifacts.mkdir(parents=True)
            v = export(lang, repo, commit)
            try:
                validate(lang, repo, artifacts, tools)
                if with_examples:
                    from wallet_validate import validate_wallet
                    validate_wallet(lang, repo, artifacts, library, example_mode)
            except (ValueError, OSError, RuntimeError) as error:
                raise RuntimeError(f'{lang} SDK validation failed: {error}') from error
            # Validate the complete generated tree, including examples and files
            # added during validation, before creating any source archive.
            hashes(repo)
            with tarfile.open(artifacts / f'{name}-{v}-source.tar.gz', 'w:gz') as archive:
                archive.add(repo, arcname=name)
            checksums = hashes(artifacts)
            write_text(artifacts / 'SHA256SUMS', ''.join(f'{digest}  {path}\n' for path, digest in checksums.items()))
            entries.append({'language': lang, 'version': v, 'repository': f'mududb/{name}',
                            'tag': f'v{v}', 'validated': True, 'files': hashes(package)})
        for entry in entries:
            name = f"mududb-{entry['language']}"
            dest = output / name
            if dest.exists():
                shutil.rmtree(dest)
            shutil.copytree(staging / name, dest)
        end_commit = run(['git', 'rev-parse', 'HEAD'], ROOT, capture=True).strip()
        dirty = dirty or end_commit != commit or bool(run(
            ['git', 'status', '--porcelain', '--untracked-files=normal'], ROOT, capture=True).strip())
        manifest = {'schema': 1, 'source_commit': commit, 'dirty': dirty,
                    'tools': versions, 'sdks': entries}
        write_text(output / 'manifest.json', json.dumps(manifest, indent=2) + '\n')
    print(f'SDK build complete: {output}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--languages', default=os.environ.get('SDK_LANGS', ','.join(CONFIG)))
    parser.add_argument('--output', type=Path, default=Path(os.environ.get('SDK_OUTPUT_DIR', ROOT / 'target/sdk-dist')))
    parser.add_argument('--with-adapter-runtime', action='store_true', help='Build the separate native adapter runtime for this host')
    parser.add_argument('--with-examples', action='store_true', help='Build and exercise selected Wallet examples (includes adapter runtime)')
    parser.add_argument('--example-mode', choices=['adapter', 'mpk', 'both'], default='both', help='Example verification mode; default both')
    args = parser.parse_args()
    try:
        build(languages(args.languages), args.output.resolve(), args.with_adapter_runtime, args.with_examples, args.example_mode)
    except (ValueError, OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'SDK build failed: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
