#!/usr/bin/env python3
"""Check C99/C++17 headers, runtime, and golden corpus without Make or a shell."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def check(cc='clang', cxx='clang++'):
    root = Path(__file__).resolve().parent
    headers = sorted((root / 'mududb/codec').glob('*.h')) + sorted((root / 'mududb/types').glob('*.h'))
    runtime = [root / 'mududb/codec' / name for name in ('mpack.c', 'mpack_alloc.c', 'record_bridge.c')]
    flags = ['-I.', '-Wall', '-Wextra', '-Werror', '-g']
    if sys.platform == 'win32':
        flags += ['-D_CRT_SECURE_NO_WARNINGS']
    def call(args):
        args = list(map(str, args))
        print('+ ' + subprocess.list2cmdline(args), flush=True)
        subprocess.run(args, cwd=root, check=True, shell=False)
    for header in headers:
        call([cc, '-x', 'c-header', '-std=c99', *flags, '-fsyntax-only', header])
    for source in headers + runtime:
        call([cxx, '-x', 'c++', '-std=c++17', *flags, '-fsyntax-only', source])
    with tempfile.TemporaryDirectory(prefix='mududb-c-check-') as tmp:
        driver = Path(tmp) / ('corpus_driver.exe' if sys.platform == 'win32' else 'corpus_driver')
        call([cc, '-std=c99', *flags, '-o', driver, 'tests/corpus_driver.c', 'tests/mini_json.c', *runtime])
        call([driver])
    print(f'C99/C++17 headers ({len(headers)}) and golden corpus OK')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cc', default=os.environ.get('CC', 'clang'))
    parser.add_argument('--cxx', default=os.environ.get('CXX', 'clang++'))
    args = parser.parse_args()
    try:
        check(args.cc, args.cxx)
    except (OSError, subprocess.CalledProcessError) as error:
        print(f'C SDK check failed in {Path(__file__).resolve().parent}: {error}', file=sys.stderr)
        sys.exit(1)
