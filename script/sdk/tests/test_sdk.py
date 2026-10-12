"""Distribution and publication regression tests: local Git and fake GitHub only."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import sdk_build as build
import publish


class SDKTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.env = os.environ.copy()
        self.env.update(GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='core.autocrlf', GIT_CONFIG_VALUE_0='false')
        self.remote = self.root / 'remote.git'
        subprocess.run(['git', 'init', '--bare', str(self.remote)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.dist = self.root / 'dist'
        self.package = self.dist / 'mududb-c'
        repo = self.package / 'repo'
        repo.mkdir(parents=True)
        (self.package / 'artifacts').mkdir()
        (self.package / 'artifacts/source.tar.gz').write_bytes(b'test artifact')
        (repo / 'README.md').write_text('SDK source\n')
        (repo / 'sdk-source.json').write_text(json.dumps({
            'source_commit': 'a' * 40, 'version': '0.1.0', 'repository': 'mududb/mududb-c'}))
        self.entry = {'language': 'c', 'repository': 'mududb/mududb-c', 'version': '0.1.0',
                      'tag': 'v0.1.0', 'validated': True, 'files': build.hashes(self.package)}
        self.manifest = {'schema': 1, 'dirty': False, 'source_commit': 'a' * 40, 'sdks': [self.entry]}
        self.write_manifest()

    def tearDown(self):
        self.tmp.cleanup()

    def write_manifest(self):
        (self.dist / 'manifest.json').write_text(json.dumps(self.manifest))

    def prepare(self, name):
        work = self.root / name
        changed = publish.prepare(self.entry, self.package, work, self.env, str(self.remote))
        return work, changed

    def push(self, work):
        publish.git(['push', '--atomic', 'origin', 'HEAD:refs/heads/main',
                     'refs/tags/' + self.entry['tag']], work, self.env)

    def test_language_validation(self):
        self.assertEqual(build.languages('py,go'), ['py', 'go'])
        for value in ('', 'py,py', 'py,wat', 'py,', '../x', 'py, go'):
            with self.assertRaises(ValueError):
                build.languages(value)

    def test_dotnet_runtime_requires_netcore_eight(self):
        for output, expected in [
            ('Microsoft.NETCore.App 10.0.12 [/snap/shared]\n', False),
            ('Microsoft.AspNetCore.App 8.0.29 [/shared]\n', False),
            ('Microsoft.NETCore.App 8.0.0-preview.1 [/shared]\n', False),
            ('Microsoft.NETCore.App 10.0.12 [/shared]\nMicrosoft.NETCore.App 8.0.29 [/shared]\n', True),
        ]:
            with self.subTest(output=output), patch.object(build.subprocess, 'run') as run:
                run.return_value = subprocess.CompletedProcess([], 0, output, '')
                available, _ = build.dotnet_runtime_check('/snap/bin/dotnet')
                self.assertEqual(available, expected)
                self.assertEqual(run.call_args.args[0], ['/snap/bin/dotnet', '--list-runtimes'])

    def test_dotnet_probe_failure_is_actionable(self):
        with patch.object(build.subprocess, 'run', side_effect=subprocess.TimeoutExpired('dotnet', 15)):
            available, details = build.dotnet_runtime_check('/snap/bin/dotnet')
        self.assertFalse(available)
        self.assertIn('timed out', details)

    @patch('sdk_build.sys.platform', 'linux')
    def test_missing_dotnet_runtime_excludes_cs_from_suggested_subset(self):
        with patch.object(build.shutil, 'which', side_effect=lambda name: '/tools/' + name), \
                patch.object(build, 'dotnet_runtime_check', return_value=(False, 'Microsoft.NETCore.App 10.0.12 [/shared]')):
            with self.assertRaises(ValueError) as error:
                build.preflight(['cs', 'c'])
        message = str(error.exception)
        self.assertIn('Current .NET host: /tools/dotnet', message)
        self.assertIn('Microsoft.NETCore.App 10.0.12', message)
        self.assertIn('--channel 8.0', message)
        self.assertIn('export PATH="$DOTNET_ROOT:$PATH"', message)
        self.assertIn('SDK_LANGS=c cargo make build-sdk', message)

    @patch('sdk_build.sys.platform', 'linux')
    def test_missing_python_build_suggests_existing_environment(self):
        interpreter = self.root / '.venv/sdk-build/bin/python3'
        interpreter.parent.mkdir(parents=True)
        interpreter.touch()
        with patch.object(build, 'ROOT', self.root), \
                patch.object(build.shutil, 'which', return_value='/tools/tool'), \
                patch.object(build, 'python_build_available', side_effect=[False, True]):
            with self.assertRaises(ValueError) as error:
                build.preflight(['py'])
        message = str(error.exception)
        self.assertIn(sys.executable, message)
        self.assertIn('source .venv/sdk-build/bin/activate', message)
        self.assertNotIn('pip install', message)
        self.assertIn('SDK_LANGS=py cargo make build-sdk', message)

    @patch('sdk_build.sys.platform', 'linux')
    def test_missing_tools_show_installation_and_available_subset(self):
        with patch.object(build, 'ROOT', self.root), \
                patch.object(build.shutil, 'which', side_effect=lambda name: None if name == 'go' else '/tools/' + name), \
                patch.object(build, 'python_build_available', return_value=False):
            with self.assertRaises(ValueError) as error:
                build.preflight(['py', 'go', 'c'])
        message = str(error.exception)
        self.assertIn('python3 -m venv .venv/sdk-build', message)
        self.assertIn('python3 -m pip install --upgrade pip build', message)
        self.assertIn('https://go.dev/doc/install', message)
        self.assertIn('SDK_LANGS=c cargo make build-sdk', message)

    @patch('sdk_build.sys.platform', 'linux')
    def test_existing_environment_without_build_suggests_installation(self):
        interpreter = self.root / '.venv/sdk-build/bin/python3'
        interpreter.parent.mkdir(parents=True)
        interpreter.touch()
        with patch.object(build, 'ROOT', self.root), \
                patch.object(build.shutil, 'which', return_value='/tools/tool'), \
                patch.object(build, 'python_build_available', return_value=False):
            with self.assertRaises(ValueError) as error:
                build.preflight(['py'])
        message = str(error.exception)
        self.assertIn('source .venv/sdk-build/bin/activate', message)
        self.assertIn('python3 -m pip install --upgrade pip build', message)
        self.assertNotIn('python3 -m venv', message)

    def test_missing_common_tool_does_not_suggest_language_subset(self):
        with patch.object(build.shutil, 'which', side_effect=lambda name: None if name == 'cargo' else '/tools/' + name):
            with self.assertRaises(ValueError) as error:
                build.preflight(['c'])
        message = str(error.exception)
        self.assertIn('https://rustup.rs/', message)
        self.assertNotIn('Or build only', message)

    def test_manifest_tampering_and_dirty(self):
        publish.load_manifest(self.dist, 'a' * 40)
        with self.assertRaises(ValueError):
            publish.load_manifest(self.dist, 'b' * 40)
        self.manifest['dirty'] = True
        self.write_manifest()
        with self.assertRaises(ValueError):
            publish.load_manifest(self.dist)
        self.manifest['dirty'] = False
        self.write_manifest()
        (self.package / 'repo/README.md').write_text('tampered')
        with self.assertRaises(ValueError):
            publish.load_manifest(self.dist)

    def test_path_and_repository_validation(self):
        self.entry['language'] = '../../x'
        self.write_manifest()
        with self.assertRaises(ValueError):
            publish.load_manifest(self.dist)

    def test_first_publish_retry_and_version_conflict(self):
        work, changed = self.prepare('first')
        self.assertTrue(changed)
        self.push(work)
        _, changed = self.prepare('retry')
        self.assertFalse(changed)
        (self.package / 'repo/README.md').write_text('different same version')
        with self.assertRaisesRegex(ValueError, 'different content'):
            self.prepare('conflict')

    def test_remote_manual_edits_rejected(self):
        work, _ = self.prepare('first')
        self.push(work)
        (work / 'manual.txt').write_text('manual edit')
        publish.git(['add', '.'], work, self.env)
        publish.git(['commit', '-m', 'manual'], work, self.env)
        publish.git(['push', 'origin', 'main'], work, self.env)
        with self.assertRaisesRegex(ValueError, 'unmanaged'):
            self.prepare('second')

    def test_concurrent_remote_push_rejected(self):
        work, _ = self.prepare('first')
        self.push(work)
        self.entry['version'], self.entry['tag'] = '0.1.1', 'v0.1.1'
        source = self.package / 'repo/sdk-source.json'
        data = json.loads(source.read_text())
        data['version'] = '0.1.1'
        source.write_text(json.dumps(data))
        second, _ = self.prepare('second')
        (work / 'concurrent.txt').write_text('changed')
        publish.git(['add', '.'], work, self.env)
        publish.git(['commit', '-m', 'concurrent'], work, self.env)
        publish.git(['push', 'origin', 'main'], work, self.env)
        with self.assertRaises(RuntimeError):
            self.push(second)
        refs = publish.git(['ls-remote', '--tags', 'origin'], second, self.env)
        self.assertNotIn('v0.1.1', refs)

    def test_fake_api_partial_failure_and_retry(self):
        class FakeAPI:
            fail = True
            def repository(self, name):
                return {'permissions': {'push': True}}
            def release(self, *args):
                if self.fail:
                    raise RuntimeError('simulated upload interruption')
        api = FakeAPI()
        real_prepare = publish.prepare
        def local_prepare(entry, package, work, env):
            return real_prepare(entry, package, work, env, str(self.remote))
        with patch.object(publish, 'prepare', side_effect=local_prepare):
            with self.assertRaises(RuntimeError):
                publish.publish(self.dist, api, self.env)
            report = json.loads((self.dist / 'publish-report.json').read_text())
            self.assertEqual(report[0]['status'], 'failed')
            api.fail = False
            publish.publish(self.dist, api, self.env)
            report = json.loads((self.dist / 'publish-report.json').read_text())
            self.assertEqual(report[0]['status'], 'published')

    def test_missing_repository_is_not_created_without_opt_in(self):
        api = unittest.mock.Mock()
        api.repository.return_value = None
        with self.assertRaisesRegex(ValueError, 'missing or inaccessible'):
            publish.publish(self.dist, api, self.env)
        api.create.assert_not_called()
        api.release.assert_not_called()

    def test_dry_run_does_not_create_repositories_or_releases(self):
        api = unittest.mock.Mock()
        api.repository.return_value = None
        publish.publish(self.dist, api, self.env, create_repos=True, dry_run=True)
        api.create.assert_not_called()
        api.release.assert_not_called()

    def test_export_excludes_caches_and_rejects_symlinks(self):
        src, dest = self.root / 'source', self.root / 'export'
        (src / 'lib/node_modules').mkdir(parents=True)
        (src / 'lib/a.py').write_text('safe')
        (src / 'lib/node_modules/b.js').write_text('cache')
        build.copy_source(src, dest, ['lib'])
        self.assertEqual(list(build.hashes(dest)), ['lib/a.py'])
        try:
            (src / 'lib/link').symlink_to(src / 'lib/a.py')
        except OSError:
            if sys.platform == 'win32':
                self.skipTest('Windows symlink test requires Developer Mode or symlink privilege')
            raise
        with self.assertRaises(ValueError):
            build.copy_source(src, dest, ['lib'])

    def test_credentials_rejected_before_copy_and_manifest_hashing(self):
        for name in ('.env', '.env.local', '.env.production', '.secrets/token',
                     '.aws/credentials', '.ssh/config', '.azure/token', '.kube/config',
                     '.npmrc', '.pypirc', '.netrc', '_netrc', 'credentials.json',
                     'service-account-prod.json', 'publish.token', 'key.pem', 'key.key',
                     'key.p12', 'key.pfx', 'key.jks', 'key.keystore', 'id_rsa',
                     'id_ed25519', 'id_dsa', 'id_ecdsa', 'KEY.PEM'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                src, dest = Path(tmp) / 'source', Path(tmp) / 'export'
                credential = src / 'lib/nested' / name
                credential.parent.mkdir(parents=True)
                credential.write_text('sensitive-fixture-content')
                for operation in (lambda: build.copy_source(src, dest, ['lib']),
                                  lambda: build.hashes(src)):
                    with self.assertRaises(ValueError) as error:
                        operation()
                    self.assertNotIn('sensitive-fixture-content', str(error.exception))
                self.assertFalse((dest / 'lib/nested' / name).exists())

    def test_environment_templates_are_exported(self):
        src, dest = self.root / 'source', self.root / 'export'
        src.mkdir()
        for name in build.ENV_TEMPLATES:
            (src / name).write_text('API_KEY=\n')
        build.copy_source(src, dest, ['.env.*'])
        self.assertEqual(set(build.hashes(dest)), build.ENV_TEMPLATES)

    def test_private_key_in_ordinary_file_is_rejected(self):
        src, dest = self.root / 'source', self.root / 'export'
        src.mkdir()
        # Assemble a dummy header without embedding a key in the test source.
        (src / 'settings.txt').write_text('-----BEGIN ' + 'PRIVATE KEY-----\nfixture')
        with self.assertRaisesRegex(ValueError, 'Private key material'):
            build.copy_source(src, dest, ['*.txt'])
        with self.assertRaisesRegex(ValueError, 'Private key material'):
            build.hashes(src)

    def test_failed_batch_invalidates_prior_manifest(self):
        with patch.object(build, 'preflight', side_effect=ValueError('tool missing')):
            with self.assertRaises(ValueError):
                build.build(['c'], self.dist)
        self.assertFalse((self.dist / 'manifest.json').exists())

    def test_credentials_never_in_helper_or_environment(self):
        path = self.root / 'token'
        path.write_text('test-token-secret\n')
        with publish.credentials(path) as (_, env):
            self.assertNotIn('test-token-secret', json.dumps(env))
            self.assertNotIn('test-token-secret', Path(env['GIT_ASKPASS']).read_text())
            helper = Path(env['GIT_ASKPASS'])
        self.assertFalse(helper.exists())

    def test_resume_draft_when_tag_lookup_returns_not_found(self):
        api = publish.GitHub('unused')
        artifact = self.package / 'artifacts/source.tar.gz'
        with patch.object(api, 'request', side_effect=[
            None,
            [{'id': 7, 'tag_name': 'v0.1.0', 'draft': True,
              'upload_url': 'https://uploads.github.com/repos/mududb/mududb-c/releases/7/assets{?name,label}'}],
            [], {'name': artifact.name}, None,
        ]) as request:
            api.release('mududb/mududb-c', 'v0.1.0', artifact.parent, 'a' * 40)
            posts = [call for call in request.call_args_list if call.args[0] == 'POST']
            self.assertEqual(len(posts), 1)
            self.assertTrue(posts[0].args[1].startswith('https://uploads.github.com/'))
            self.assertEqual(request.call_args.args[0], 'PATCH')

    def test_release_resume_checks_asset_digest(self):
        import hashlib
        file = self.package / 'artifacts/source.tar.gz'
        digest = 'sha256:' + hashlib.sha256(file.read_bytes()).hexdigest()
        api = publish.GitHub('unused')
        with patch.object(api, 'request', side_effect=[
            {'id': 1, 'draft': True, 'upload_url': 'unused'},
            [{'name': file.name, 'digest': digest}], None,
        ]) as request:
            api.release('mududb/mududb-c', 'v0.1.0', file.parent, 'a' * 40)
            self.assertEqual(request.call_args.args[0], 'PATCH')
        with patch.object(api, 'request', side_effect=[
            {'id': 1, 'draft': False}, [{'name': file.name, 'digest': 'different'}],
        ]):
            with self.assertRaises(RuntimeError):
                api.release('mududb/mududb-c', 'v0.1.0', file.parent, 'a' * 40)


if __name__ == '__main__':
    unittest.main()
