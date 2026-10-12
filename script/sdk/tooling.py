"""Portable process and path helpers for SDK build tooling."""
from pathlib import Path
import subprocess
import sys


def executable_name(name, platform=None):
    return name + ('.exe' if (platform or sys.platform) == 'win32' else '')


def venv_python(root, platform=None):
    return Path(root) / ('Scripts/python.exe' if (platform or sys.platform) == 'win32' else 'bin/python3')


def write_text(path, text):
    Path(path).write_text(text, encoding='utf-8', newline='\n')


def run(args, cwd, env=None, capture=False, timeout=None):
    """Run a native executable, retaining context when a child fails."""
    args = list(map(str, args))
    print('+ ' + subprocess.list2cmdline(args), flush=True)
    try:
        return subprocess.run(args, cwd=cwd, env=env, check=True, shell=False,
                              text=True, encoding='utf-8', errors='replace',
                              stdout=subprocess.PIPE if capture else None,
                              timeout=timeout).stdout
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise RuntimeError(f'Command failed in {cwd}: {subprocess.list2cmdline(args)}\n{error}') from error


def npm_command(node, npm):
    """Resolve npm's JS entry point without executing npm.cmd or a shell shim."""
    candidates = []
    for executable in (npm, node):
        path = Path(executable)
        resolved = path.resolve()
        if resolved.name == 'npm-cli.js':
            candidates.append(resolved)
        for parent in (path.parent, resolved.parent):
            candidates.extend([parent / 'node_modules/npm/bin/npm-cli.js',
                               parent.parent / 'lib/node_modules/npm/bin/npm-cli.js',
                               parent.parent / 'share/nodejs/npm/bin/npm-cli.js'])
    for candidate in candidates:
        if candidate.is_file():
            return [node, str(candidate.resolve())]
    raise ValueError('Cannot locate npm-cli.js beside Node/npm. Install Node.js with npm '
                     'from https://nodejs.org/en/download and put that installation on PATH. '
                     f'Found node={node}, npm={npm}')
