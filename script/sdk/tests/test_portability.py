"""Regression coverage for native process invocation and portable SDK checks."""
import importlib.util
import io
from contextlib import redirect_stdout
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import sdk_build
import tooling

spec = importlib.util.spec_from_file_location('regen', Path(__file__).resolve().parents[2] / 'ci/check_bindings_regen.py')
regen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(regen)


class PortabilityTests(unittest.TestCase):
    def test_process_arguments_and_cwd_are_literal(self):
        with tempfile.TemporaryDirectory(prefix='sdk space 中文 ') as tmp:
            marker = Path(tmp) / 'result.txt'
            value = 'spaces & $(echo injected) ; 中文'
            tooling.run([sys.executable, '-c',
                         'import pathlib,sys; pathlib.Path("result.txt").write_text(sys.argv[1],encoding="utf-8")', value], tmp)
            self.assertEqual(marker.read_text(encoding='utf-8'), value)

    def test_command_failure_reports_working_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(RuntimeError) as error:
                tooling.run([sys.executable, '-c', 'raise SystemExit(7)'], tmp)
            self.assertIn(tmp, str(error.exception))
            self.assertIn('7', str(error.exception))

    def test_executable_and_venv_layouts(self):
        root = Path('a space')
        self.assertEqual(tooling.venv_python(root, 'win32'), root / 'Scripts/python.exe')
        self.assertEqual(tooling.venv_python(root, 'darwin'), root / 'bin/python3')
        self.assertEqual(tooling.executable_name('mgen', 'win32'), 'mgen.exe')
        self.assertEqual(tooling.executable_name('mgen', 'linux'), 'mgen')

    def test_npm_native_layouts_without_executing_shims(self):
        for relative in ['node_modules/npm/bin/npm-cli.js', '../lib/node_modules/npm/bin/npm-cli.js',
                         '../share/nodejs/npm/bin/npm-cli.js']:
            with self.subTest(relative=relative), tempfile.TemporaryDirectory(prefix='node space ') as tmp:
                binary = Path(tmp) / 'bin'
                binary.mkdir()
                cli = binary / relative
                cli.parent.mkdir(parents=True)
                cli.write_text('// npm fixture', encoding='utf-8')
                self.assertEqual(tooling.npm_command(str(binary / 'node.exe'), str(binary / 'npm.cmd')),
                                 [str(binary / 'node.exe'), str(cli.resolve())])

    def test_missing_npm_cli_has_installation_hint(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, 'Install Node.js with npm'):
                tooling.npm_command(str(Path(tmp) / 'node'), str(Path(tmp) / 'npm'))

    def test_windows_diagnostic_has_no_unix_commands(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(sdk_build, 'ROOT', Path(tmp)), \
                patch.object(sys, 'platform', 'win32'):
            message = sdk_build.missing_tool_message(['Python build', 'dotnet', 'clang', 'go'],
                                                    ['py', 'cs', 'c', 'go'],
                                                    {'py': [], 'cs': ['dotnet'], 'c': ['clang'], 'go': ['go']})
        self.assertIn('Activate.ps1', message)
        self.assertIn('$env:SDK_LANGS', message)
        self.assertIn('MSVC Build Tools', message)
        for forbidden in ['source ', 'export ', 'sudo ', 'bash ', '/tmp/mududb-dotnet']:
            self.assertNotIn(forbidden, message)

    def test_generated_text_is_utf8_lf(self):
        with tempfile.TemporaryDirectory() as tmp:
            file = Path(tmp) / 'text.txt'
            tooling.write_text(file, '中文\n')
            self.assertEqual(file.read_bytes(), '中文\n'.encode('utf-8'))

    def test_linux_generator_uses_native_linker_and_preserves_flags(self):
        with tempfile.TemporaryDirectory(prefix='rust sysroot ') as tmp:
            host = 'x86_64-unknown-linux-gnu'
            lld = Path(tmp) / 'lib/rustlib' / host / 'bin/gcc-ld'
            lld.mkdir(parents=True)
            with patch.object(regen, 'run', side_effect=[f'host: {host}\n', tmp]), \
                    patch.object(regen.shutil, 'which', return_value='/usr/bin/cc'), \
                    patch.dict(os.environ, {'CARGO_ENCODED_RUSTFLAGS': '--cfg\x1ffeature="a b"'}, clear=True):
                env, actual_host = regen.generator_environment(Path(tmp))
            self.assertEqual(actual_host, host)
            self.assertEqual(env['CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER'], '/usr/bin/cc')
            flags = env['CARGO_ENCODED_RUSTFLAGS'].split('\x1f')
            self.assertEqual(flags[:2], ['--cfg', 'feature="a b"'])
            self.assertIn(f'link-arg=-B{lld}', flags)
            self.assertIn('link-arg=-fuse-ld=lld', flags)

    def test_regen_reports_missing_and_byte_differences(self):
        with tempfile.TemporaryDirectory() as tmp:
            file = Path(tmp) / 'generated.ts'
            self.assertFalse(regen.compare(file, b'code\n'))
            file.write_bytes(b'code\r\n')
            self.assertFalse(regen.compare(file, b'code\n'))
            file.write_bytes(b'code\n')
            self.assertTrue(regen.compare(file, b'code\n'))

    def test_regen_distinguishes_line_endings_from_content_changes(self):
        cases = [
            (b'code\n', b'code\r\n', False, True),
            (b'code\r\n', b'code\n', False, True),
            (b'a\nb\n', b'a\r\nb\n', False, True),
            (b'old\r\n', b'new\n', False, False),
            (b'code\n', b'code \n', False, False),
            (b'code\n', b'code\n', True, False),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            file = Path(tmp) / 'generated.ts'
            for checked, generated, expected, newline_hint in cases:
                with self.subTest(checked=checked, generated=generated):
                    file.write_bytes(checked)
                    output = io.StringIO()
                    with redirect_stdout(output):
                        self.assertEqual(regen.compare(file, generated), expected)
                    self.assertEqual('CRLF/LF-only difference' in output.getvalue(), newline_hint)
                    self.assertEqual('STALE:' in output.getvalue(), not expected)

    def test_generator_inputs_checkout_as_lf_with_autocrlf_enabled(self):
        root = Path(__file__).resolve().parents[3]
        templates = Path('crates/tools/mudu_gen/templates')
        template_sources = sorted((root / templates).rglob('*.jinja'))
        wit_sources = sorted((root / 'crates/common/mudu_binding/wit').rglob('*.wit'))
        self.assertTrue(template_sources)
        self.assertTrue(wit_sources)
        sources = template_sources + wit_sources
        with tempfile.TemporaryDirectory() as tmp:
            checkout = Path(tmp)
            env = dict(os.environ, GIT_CONFIG_COUNT='0')

            def git(*args):
                return subprocess.run(['git', '-c', 'core.autocrlf=true',
                                       '-c', 'core.safecrlf=false', *args],
                                      cwd=checkout, env=env, check=True, capture_output=True)

            git('init')
            (checkout / '.gitattributes').write_bytes((root / '.gitattributes').read_bytes())
            for source in sources:
                dest = checkout / source.relative_to(root)
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(source.read_bytes())
            git('add', '.')
            for source in sources:
                (checkout / source.relative_to(root)).unlink()
            git('checkout-index', '--all', '--force')
            for source in sources:
                content = (checkout / source.relative_to(root)).read_bytes()
                self.assertNotIn(b'\r', content, str(source))
                self.assertEqual(content, source.read_bytes().replace(b'\r\n', b'\n'))

    def test_regen_failure_cleans_temporary_directory(self):
        directories = []
        def fake_run(args, *unused, **kwargs):
            if args[0] == 'cargo' and args[1] == 'metadata':
                return '{"target_directory":"/unused"}'
            if len(args) > 1 and args[1] == 'message':
                directories.append(Path(args[args.index('-o') + 1]).parent)
                raise RuntimeError('generator failed')
        with patch.object(regen, 'generator_environment', return_value=({}, 'test-host')), \
                patch.object(regen, 'run', side_effect=fake_run):
            with self.assertRaisesRegex(RuntimeError, 'generator failed'):
                regen.check()
        self.assertEqual(len(directories), 1)
        self.assertFalse(directories[0].exists())


if __name__ == '__main__':
    unittest.main()
