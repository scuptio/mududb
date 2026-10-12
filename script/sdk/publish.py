#!/usr/bin/env python3
"""Publish verified SDK artifacts to GitHub; called only by the manual Action."""
import argparse
import hashlib
import re
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

from sdk_build import CONFIG, hashes


class GitHub:
    """Small GitHub API client; credentials never appear in commands or errors."""
    def __init__(self, token):
        self.token = token

    def request(self, method, path, data=None, binary=False):
        url = path if path.startswith('https://uploads.github.com/') else 'https://api.github.com' + path
        body = data if binary else (json.dumps(data).encode() if data is not None else None)
        req = urllib.request.Request(url, data=body, method=method, headers={
            'Authorization': f'Bearer {self.token}', 'Accept': 'application/vnd.github+json',
            'Content-Type': 'application/octet-stream' if binary else 'application/json',
            'X-GitHub-Api-Version': '2022-11-28',
        })
        try:
            with urllib.request.urlopen(req, timeout=120) as response:
                content = response.read()
                return json.loads(content) if content else None
        except urllib.error.HTTPError as error:
            if error.code == 404 and method == 'GET':
                return None
            raise RuntimeError(f'GitHub {method} request failed with HTTP {error.code}') from None
        except urllib.error.URLError:
            raise RuntimeError('GitHub request failed; check connectivity and retry') from None

    def repository(self, name):
        return self.request('GET', '/repos/' + name)

    def create(self, name):
        return self.request('POST', '/orgs/mududb/repos', {
            'name': name.split('/')[1], 'private': False, 'auto_init': False,
            'description': 'Generated MuduDB SDK distribution; contribute in scuptio/mududb',
        })

    def release(self, name, tag, artifacts, commit, body=None):
        base = '/repos/' + name
        release = self.request('GET', base + '/releases/tags/' + tag)
        if release is None:
            # Tag lookup is for published releases. Enumerate drafts as well so
            # interrupted uploads can resume without creating a duplicate release.
            page = 1
            while True:
                candidates = self.request('GET', base + f'/releases?per_page=100&page={page}')
                release = next((item for item in candidates if item['tag_name'] == tag), None)
                if release is not None or len(candidates) < 100:
                    break
                page += 1
        if release is None:
            release = self.request('POST', base + '/releases', {
                'tag_name': tag, 'name': tag, 'draft': True,
                'body': body or f'Generated from scuptio/mududb commit {commit}. See sdk-source.json and SHA256SUMS.',
            })
        assets = []
        page = 1
        while True:
            batch = self.request('GET', base + f"/releases/{release['id']}/assets?per_page=100&page={page}")
            assets.extend(batch)
            if len(batch) < 100:
                break
            page += 1
        existing = {item['name']: item for item in assets}
        for file in sorted(artifacts.iterdir()):
            expected = 'sha256:' + hashlib.sha256(file.read_bytes()).hexdigest()
            if file.name in existing:
                asset = existing[file.name]
                if asset.get('digest') != expected:
                    raise RuntimeError(f'Existing Release asset differs or has no verified digest: {file.name}')
                continue
            upload = release['upload_url'].split('{')[0] + '?name=' + urllib.parse.quote(file.name)
            self.request('POST', upload, file.read_bytes(), binary=True)
        if release['draft']:
            self.request('PATCH', base + f"/releases/{release['id']}", {'draft': False})


def git(args, cwd, env):
    result = subprocess.run(['git', *args], cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        # Do not forward transport output: credential helpers may include sensitive information.
        raise RuntimeError(f'Git operation failed: {args[0]} (exit {result.returncode})')
    return result.stdout.strip()


@contextmanager
def credentials(token_file):
    """Use an ephemeral askpass helper; persist neither token nor git configuration."""
    token = token_file.read_text().strip()
    if not token or any(c.isspace() for c in token):
        raise ValueError('Token file must contain one nonempty token')
    with tempfile.TemporaryDirectory(prefix='sdk-auth-') as tmp:
        helper = Path(tmp) / 'askpass'
        helper.write_text('#!/usr/bin/env python3\nimport os, sys\nfrom pathlib import Path\n'
                          'print("x-access-token" if "username" in sys.argv[1].lower() '
                          'else Path(os.environ["SDK_GITHUB_TOKEN_FILE"]).read_text().strip())\n')
        helper.chmod(0o700)
        env = os.environ.copy()
        env.pop('SDK_PUBLISH_TOKEN', None)
        env.pop('GITHUB_TOKEN', None)
        env.pop('GH_TOKEN', None)
        env.update(GIT_ASKPASS=str(helper), GIT_TERMINAL_PROMPT='0',
                   SDK_GITHUB_TOKEN_FILE=str(token_file.resolve()),
                   GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='credential.helper', GIT_CONFIG_VALUE_0='')
        yield GitHub(token), env


def load_manifest(directory, expected_commit=None):
    manifest = json.loads((directory / 'manifest.json').read_text())
    if manifest.get('schema') != 1 or manifest.get('dirty') is not False:
        raise ValueError('Publishing requires a schema-1 build from a clean source commit')
    if expected_commit and manifest['source_commit'] != expected_commit:
        raise ValueError('Build source commit does not match the workflow commit')
    entries = manifest.get('sdks', [])
    if not entries:
        raise ValueError('Manifest contains no SDKs')
    seen = set()
    for entry in entries:
        lang = entry['language']
        if lang not in CONFIG or lang in seen or entry['repository'] != f'mududb/mududb-{lang}':
            raise ValueError('Invalid or duplicate SDK repository in manifest')
        seen.add(lang)
        if not re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', entry['version']):
            raise ValueError('Invalid SDK version')
        if entry['tag'] != 'v' + entry['version'] or entry.get('validated') is not True:
            raise ValueError('Invalid SDK tag or incomplete validation')
        package = directory / f'mududb-{lang}'
        if hashes(package) != entry['files'] or not (package / 'repo/sdk-source.json').is_file():
            raise ValueError(f'SDK content missing or modified after validation: {lang}')
        source = json.loads((package / 'repo/sdk-source.json').read_text())
        if any(source.get(key) != value for key, value in {
            'source_commit': manifest['source_commit'], 'version': entry['version'],
            'repository': entry['repository']}.items()):
            raise ValueError('SDK source metadata does not match manifest')
        if (package / 'repo/.git').exists() or any(p.startswith('repo/.github/') for p in entry['files']):
            raise ValueError('Distribution cannot inject Git metadata or workflows')
    return manifest


def tree_hashes(repo):
    return {p: h for p, h in hashes_without_git(repo).items() if p != '.sdk-managed.json'}


def hashes_without_git(repo):
    result = {}
    for path in repo.rglob('*'):
        rel = path.relative_to(repo)
        if '.git' in rel.parts:
            continue
        if path.is_symlink():
            raise ValueError('Symlink in remote SDK tree')
        if path.is_file():
            result[rel.as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def prepare(entry, package, work, env, remote=None):
    """Prepare a fast-forward update and immutable tag, without touching the remote."""
    url = remote or f"https://github.com/{entry['repository']}.git"
    work.mkdir()
    git(['init', '-b', 'main'], work, env)
    git(['remote', 'add', 'origin', url], work, env)
    git(['config', 'user.name', 'MuduDB SDK Publisher'], work, env)
    git(['config', 'user.email', 'sdk-publisher@users.noreply.github.com'], work, env)
    refs = git(['ls-remote', '--heads', '--tags', 'origin'], work, env)
    has_main = any(line.endswith('refs/heads/main') for line in refs.splitlines())
    if refs and not has_main:
        raise ValueError('Existing distribution repository must have a managed main branch')
    if refs:
        git(['fetch', 'origin', 'main', '--tags'], work, env)
        git(['reset', '--hard', 'refs/remotes/origin/main'], work, env)
        marker = work / '.sdk-managed.json'
        if not marker.exists() or json.loads(marker.read_text()) != tree_hashes(work):
            raise ValueError(f"Remote contains unmanaged changes: {entry['repository']}")
    expected = hashes(package / 'repo')
    tag = entry['tag']
    exists = any(line.endswith('refs/tags/' + tag) for line in refs.splitlines())
    if exists:
        # Check the full old tag tree, not just a mutable source metadata file.
        git(['checkout', '--detach', tag], work, env)
        if tree_hashes(work) != expected:
            raise ValueError(f"Version already exists with different content: {entry['repository']} {tag}")
        marker = work / '.sdk-managed.json'
        if not marker.exists() or json.loads(marker.read_text()) != expected:
            raise ValueError('Existing version is not a managed SDK release')
        return False
    for path in work.iterdir():
        if path.name != '.git':
            if path.is_dir():
                shutil.rmtree(path)
            else:
                path.unlink()
    shutil.copytree(package / 'repo', work, dirs_exist_ok=True)
    (work / '.sdk-managed.json').write_text(json.dumps(expected, indent=2) + '\n')
    git(['add', '--all'], work, env)
    source = json.loads((package / 'repo/sdk-source.json').read_text())
    git(['commit', '-m', f"SDK {tag} from {source['source_commit']}"], work, env)
    git(['tag', tag], work, env)
    return True


def publish(directory, api, env, create_repos=False, dry_run=False, expected_commit=None):
    manifest = load_manifest(directory, expected_commit)
    report = []
    report_path = directory / 'publish-report.json'
    with tempfile.TemporaryDirectory(prefix='sdk-publish-') as tmp:
        missing = []
        prepared = []
        # Check every existing repository and tag before the first remote write.
        for entry in manifest['sdks']:
            info = api.repository(entry['repository'])
            work = Path(tmp) / ('mududb-' + entry['language'])
            if info is None:
                if not create_repos:
                    raise ValueError(f"Repository missing or inaccessible: {entry['repository']}")
                missing.append((entry, work))
                continue
            if not info.get('permissions', {}).get('push') or info.get('archived') or info.get('private'):
                raise ValueError(f"Need a writable, public, non-archived repository: {entry['repository']}")
            changed = prepare(entry, directory / work.name, work, env)
            prepared.append((entry, work, changed))
        if dry_run:
            print(json.dumps({'publish': [e['repository'] for e in manifest['sdks']],
                              'create': [e['repository'] for e, _ in missing]}, indent=2))
            return
        for entry, work in missing:
            api.create(entry['repository'])
            changed = prepare(entry, directory / work.name, work, env)
            prepared.append((entry, work, changed))
        try:
            for entry, work, changed in prepared:
                record = {'repository': entry['repository'], 'tag': entry['tag'], 'status': 'failed'}
                report.append(record)
                if changed:
                    git(['push', '--atomic', 'origin', 'HEAD:refs/heads/main',
                         'refs/tags/' + entry['tag']], work, env)
                api.release(entry['repository'], entry['tag'], directory / work.name / 'artifacts',
                            manifest['source_commit'])
                record['status'] = 'published'
        finally:
            report_path.write_text(json.dumps(report, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dist', type=Path, required=True)
    parser.add_argument('--expected-commit')
    parser.add_argument('--create-repos', action='store_true')
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()
    try:
        token_file = Path(os.environ.get('SDK_GITHUB_TOKEN_FILE', '.secrets/github-sdk.token'))
        with credentials(token_file) as (api, env):
            publish(args.dist.resolve(), api, env, args.create_repos, args.dry_run, args.expected_commit)
    except (ValueError, OSError, RuntimeError, KeyError) as error:
        print(f'SDK publish failed: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
