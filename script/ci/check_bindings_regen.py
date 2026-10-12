#!/usr/bin/env python3
"""Regenerate bindings into a temporary directory and compare bytes with the checkout."""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'sdk'))
from tooling import executable_name, run

ROOT = Path(__file__).resolve().parents[2]


def generator_environment(root):
    """Replace the Linux shell linker wrapper for this build only, retaining lld."""
    env = os.environ.copy()
    host = next(line.split(': ', 1)[1] for line in
                run(['rustc', '-vV'], root, capture=True).splitlines() if line.startswith('host: '))
    if host == 'x86_64-unknown-linux-gnu':
        cc = shutil.which('cc')
        if not cc:
            raise ValueError('Building mgen requires a native C compiler (install build-essential on Ubuntu).')
        env['CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'] = cc
        sysroot = Path(run(['rustc', '--print', 'sysroot'], root, capture=True).strip())
        lld = sysroot / 'lib/rustlib' / host / 'bin/gcc-ld'
        if lld.is_dir():
            # Cargo's plain RUSTFLAGS uses whitespace splitting, not shell quoting.
            flags = (env['CARGO_ENCODED_RUSTFLAGS'].split('\x1f')
                     if env.get('CARGO_ENCODED_RUSTFLAGS') else env.get('RUSTFLAGS', '').split())
            flags += ['-C', f'link-arg=-B{lld}', '-C', 'link-arg=-fuse-ld=lld']
            env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
    return env, host


def compare(checked, generated):
    if not checked.is_file():
        print(f'MISSING: {checked}')
        return False
    content = checked.read_bytes()
    if content != generated:
        print(f'STALE: {checked}')
        if content.replace(b'\r\n', b'\n') == generated.replace(b'\r\n', b'\n'):
            print('  CRLF/LF-only difference: check .gitattributes LF rules for mgen '
                  'templates, WIT inputs and generated bindings; re-checkout the affected files '
                  'and rebuild mgen.')
        return False
    return True


def check(root=ROOT):
    workspace = root / 'crates/tools'
    env, host = generator_environment(root)
    run(['cargo', 'build', '-q', '-p', 'mudu_gen', '--bin', 'mgen', '--target', host], workspace, env)
    metadata = json.loads(run(['cargo', 'metadata', '--no-deps', '--format-version', '1'], workspace, env, capture=True))
    mgen = Path(metadata['target_directory']) / host / 'debug' / executable_name('mgen')
    wit = root / 'crates/common/mudu_binding/wit'
    specifications = [
        ('csharp', 'Uni*.cs', 'crates/sdk/mudu_api/csharp/uni'),
        ('python', '*.py', 'crates/sdk/bindings/python/mududb/generated'),
        ('assemblyscript', '*.ts', 'crates/sdk/bindings/assemblyscript/assembly/generated'),
        ('go', '*.go', 'crates/sdk/bindings/go/types'),
        ('c', '*.h', 'crates/sdk/bindings/c/mududb/types'),
    ]
    clean = True
    with tempfile.TemporaryDirectory(prefix='sdk-regen-') as tmp:
        for language, pattern, destination in specifications:
            output = Path(tmp) / language
            run([mgen, 'message', '-i', wit, '-o', output, '-l', language, '--with-func-codec'], root, env)
            files = sorted(output.glob(pattern))
            if not files:
                raise ValueError(f'mgen produced no {pattern} files for {language}')
            for file in files:
                checked = root / destination / file.name
                if language == 'csharp' and file.name == 'UniSyscall.cs':
                    checked = root / 'crates/sdk/mudu_api/csharp/mudu_sys' / file.name
                content = file.read_bytes()
                if language == 'assemblyscript':
                    content = content.replace(b'from "./mpack"', b'from "../mpack"')
                clean = compare(checked, content) and clean
        clean = compare(root / 'crates/sdk/bindings/python/mududb/generated/syscall_schema.desc.json',
                        (root / 'crates/common/mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json').read_bytes()) and clean
    if not clean:
        raise ValueError('Generated binding files are stale. Regenerate with mgen and commit the results '
                         '(doc/dev/binding_api_surface.md, Regeneration).')
    print('regen check: all generated binding files are up to date')


if __name__ == '__main__':
    try:
        check()
    except (ValueError, OSError, RuntimeError) as error:
        print(f'Regeneration check failed: {error}', file=sys.stderr)
        sys.exit(1)
