"""Build and archive the native runtime independently of pure-language SDKs."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[2]

def build_runtime(output):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    for tool in ('cargo', 'node', 'clang'):
        if not shutil.which(tool):
            raise RuntimeError(f'Native adapter build requires {tool}; install it and add it to PATH')
    target = Path(os.environ.get('SDK_ADAPTER_TARGET_DIR', ROOT/'target/sdk-adapter')).resolve()
    import sys
    sys.path.insert(0, str(ROOT/"script/ci"))
    from check_bindings_regen import generator_environment
    env, _ = generator_environment(ROOT)
    env['CARGO_TARGET_DIR'] = str(target)
    env['CARGO_BUILD_JOBS'] = '4'
    # The standalone cdylib workspace uses panic=unwind and never shares artifacts
    # with the repository's rlib-only workspace builds.
    subprocess.run(['cargo', 'build', '--locked', '--release', '--manifest-path',
                    str(ROOT/'crates/bindings/mudu_adapter_ffi/Cargo.toml')], env=env, check=True)
    name = {'Linux':'libmudu_adapter_ffi.so', 'Darwin':'libmudu_adapter_ffi.dylib', 'Windows':'mudu_adapter_ffi.dll'}[platform.system()]
    triple = next(line.split(': ',1)[1] for line in subprocess.check_output(['rustc','-vV'],text=True).splitlines() if line.startswith('host:'))
    version = tomllib.loads((ROOT/'crates/bindings/mudu_adapter_ffi/Cargo.toml').read_text())['package']['version']
    destination = output / f'mudu-adapter-{version}-{triple}'
    destination.mkdir(exist_ok=True)
    shutil.copyfile(target/'release'/name, destination/name)
    shutil.copyfile(ROOT/'LICENSE', destination/'LICENSE')
    shutil.copyfile(ROOT/'crates/bindings/mudu_adapter_ffi/include/mudu_adapter.h', destination/'mudu_adapter.h')
    node_version = subprocess.check_output(['node','--version'],text=True).strip()
    include = Path(os.environ.get('NODE_INCLUDE_DIR', '/usr/include/node'))
    with tempfile.TemporaryDirectory(prefix='mudu-node-headers-') as temporary:
        temporary = Path(temporary)
        if not (include/'node_api.h').is_file():
            archive = temporary/'headers.tar.gz'
            urllib.request.urlretrieve(f'https://nodejs.org/dist/{node_version}/node-{node_version}-headers.tar.gz', archive)
            with tarfile.open(archive) as tar:
                tar.extractall(temporary, filter='data')
            include = temporary/f'node-{node_version}/include/node'
        command = ['clang', '-shared', '-Wall', '-Wextra', '-Werror', f'-I{include}',
            f'-I{ROOT / "crates/sdk/bindings/c/mududb/adapter"}',
            str(ROOT/'crates/bindings/mudu_adapter_ffi/node/adapter.c'), '-o', str(destination/'mudu_adapter.node')]
        if platform.system() == 'Windows':
            arch = subprocess.check_output(['node','-p','process.arch'],text=True).strip()
            node_lib = temporary/'node.lib'
            urllib.request.urlretrieve(f'https://nodejs.org/dist/{node_version}/win-{arch}/node.lib', node_lib)
            command += [str(node_lib)]
        else:
            command += ['-fPIC']
            command += ['-Wl,-undefined,dynamic_lookup'] if platform.system() == 'Darwin' else ['-ldl']
        subprocess.run(command, check=True)
    metadata = {'dirty': bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=normal'],cwd=ROOT,text=True).strip()), 'version': version, 'abi': 1, 'target': triple, 'library': name,
                'node': node_version, 'operations': ['open', 'close', 'query', 'command', 'batch'], 'query_results': 'materialized', 'source_commit': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()}
    (destination/'runtime.json').write_text(json.dumps(metadata,indent=2)+'\n',encoding='utf-8')
    (destination/'README.md').write_text('Set MUDU_ADAPTER_LIBRARY to the absolute path of '+name+
        '. Set MUDU_CONNECTION before starting the process. See the SDK Wallet adapter README.\n',encoding='utf-8')
    package = destination.parent / (destination.name + '.tar.gz')
    with tarfile.open(package, 'w:gz') as tar:
        tar.add(destination, arcname=destination.name)
    digest = hashlib.sha256(package.read_bytes()).hexdigest()
    package.with_suffix(package.suffix+'.sha256').write_text(f'{digest}  {package.name}\n',encoding='utf-8')
    return destination/name
