"""Verify the transpiler runs without third-party Python packages."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'script/build/transpiler.py'


class TranspilerTests(unittest.TestCase):
    def test_wallet_config_and_manifest_without_site_packages(self):
        config = ROOT / 'example/mpk/wallet/build-cfg/transpiler-cfg.toml'
        with tempfile.TemporaryDirectory(prefix='transpiler ') as directory:
            folder = Path(directory)
            source = folder / 'source'
            source.mkdir()
            artifacts = folder / 'artifacts'
            # -S disables site-packages, reproducing a clean Python installation.
            # Empty input exercises configuration/manifest generation without Rust tools.
            result = subprocess.run([
                sys.executable, '-S', str(SCRIPT), '--config', str(config),
                '--source', str(source), '--target', str(folder / 'generated'),
                '--artifact', str(artifacts),
            ], text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            saved = json.loads((artifacts / 'cfg/transpiler-cfg.json').read_text())
            expected = tomllib.loads(config.read_text())
            self.assertEqual(saved, expected)
            manifest = json.loads((artifacts / 'transpilation-manifest.json').read_text())
            self.assertEqual(manifest['config'], saved)
            self.assertFalse((artifacts / 'cfg/transpiler-cfg.toml').exists())


if __name__ == '__main__':
    unittest.main()
