#!/usr/bin/env python3
"""Unit tests for script/release tooling (pack/publish verification logic)."""
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "lib"))

import output
import pack_registry
import publish


class ManifestVerificationTest(unittest.TestCase):
    def make_dist(self, tamper=False):
        tmp = tempfile.TemporaryDirectory()
        dist = Path(tmp.name)
        artifact = dist / "npm" / "mududb-0.1.0.tgz"
        artifact.parent.mkdir(parents=True)
        artifact.write_bytes(b"payload")
        digest = hashlib.sha256(b"tampered" if tamper else b"payload").hexdigest()
        manifest = {
            "schema": 1,
            "target": "npm",
            "name": "@mududb/mududb",
            "version": "0.1.0",
            "tag": "as-v0.1.0",
            "artifacts": [{"file": artifact.name, "sha256": digest}],
        }
        (dist / "npm.manifest.json").write_text(json.dumps(manifest))
        self.addCleanup(tmp.cleanup)
        return dist

    def test_manifest_roundtrip_ok(self):
        dist = self.make_dist()
        manifest = publish.load_registry_manifest(dist, "npm")
        self.assertEqual(manifest["version"], "0.1.0")

    def test_manifest_detects_tampered_artifact(self):
        dist = self.make_dist(tamper=True)
        with self.assertRaises(ValueError):
            publish.load_registry_manifest(dist, "npm")

    def test_manifest_requires_pack_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                publish.load_registry_manifest(Path(tmp), "npm")


class TagVersionTest(unittest.TestCase):
    def manifest(self, target="npm", version="0.1.0", tag="as-v0.1.0"):
        return {"target": target, "name": "x", "version": version, "tag": tag}

    def test_tag_from_manifest(self):
        tag = publish.check_version_and_tag(self.manifest(), None)
        self.assertEqual(tag, "as-v0.1.0")

    def test_tag_override_must_match(self):
        with self.assertRaises(ValueError):
            publish.check_version_and_tag(self.manifest(), "as-v9.9.9")

    def test_invalid_semver_rejected(self):
        with self.assertRaises(ValueError):
            publish.check_version_and_tag(self.manifest(version="1.0"), None)

    def test_source_tree_drift_rejected(self):
        # The packed version predates a source bump; verification must fail.
        with self.assertRaises(ValueError):
            publish.check_version_and_tag(self.manifest(version="9.9.9", tag="as-v9.9.9"), None)

    def test_tag_prefixes_match_workflow(self):
        # .github/workflows/bindings.yaml triggers on as-v* / cs-v* / py-v*.
        self.assertEqual(pack_registry.TAG_PREFIXES["npm"], "as-v")
        self.assertEqual(pack_registry.TAG_PREFIXES["nuget"], "cs-v")
        self.assertEqual(pack_registry.TAG_PREFIXES["pypi"], "py-v")


class SourceMetadataTest(unittest.TestCase):
    def test_versions_and_names_readable(self):
        versions = pack_registry.read_versions()
        names = pack_registry.read_names()
        for target in ("npm", "nuget", "pypi"):
            self.assertRegex(versions[target], r"^\d+\.\d+\.\d+$")
            self.assertTrue(names[target])
        self.assertEqual(names["npm"], "@mududb/mududb")
        self.assertEqual(names["nuget"], "mududb")
        self.assertEqual(names["pypi"], "mududb")


class OutputStyleTest(unittest.TestCase):
    def capture_status(self, verb, message, enabled):
        import io
        from contextlib import redirect_stdout

        output.set_enabled(enabled)
        try:
            buffer = io.StringIO()
            with redirect_stdout(buffer):
                output.status(verb, message)
            return buffer.getvalue()
        finally:
            output.set_enabled(False)

    def test_plain_when_disabled(self):
        line = self.capture_status("Installing", "mcli", enabled=False)
        self.assertEqual(line, "  Installing mcli\n")
        self.assertNotIn("\033[", line)

    def test_colored_right_aligned_when_enabled(self):
        line = self.capture_status("Installing", "mcli", enabled=True)
        self.assertIn("\033[", line)
        self.assertTrue(line.endswith(" mcli\n"))
        # Verb is right-aligned to the 12-column cargo-style gutter.
        self.assertIn(f"{'Installing':>12}", line.replace("\033[1m", "").replace("\033[32m", "").replace("\033[0m", ""))

    def test_status_kind_pads_before_coloring(self):
        output.set_enabled(True)
        try:
            rendered = output.status_kind("installed", 12)
            self.assertTrue(rendered.startswith("\033["))
            self.assertIn("installed   ", rendered.replace("\033[32m", "").replace("\033[1m", "").replace("\033[0m", ""))
        finally:
            output.set_enabled(False)

    def test_format_duration(self):
        self.assertEqual(output.format_duration(2.49), "2.5s")
        self.assertEqual(output.format_duration(141.3), "2m 21s")


if __name__ == "__main__":
    unittest.main()
