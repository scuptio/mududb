"""Publish verified platform runtime attachments to the main repository (manual CI only)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tarfile
import tempfile
from publish import GitHub


def publish_runtime(root, repository, commit, token):
    if not re.fullmatch(r'[0-9a-f]{40}',commit):raise ValueError('Expected a full source commit')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+',repository):raise ValueError('Invalid source repository')
    archives=list(Path(root).rglob('*.tar.gz'))
    if not archives:raise ValueError('No adapter runtime archives')
    targets=set();versions=set()
    with tempfile.TemporaryDirectory(prefix='runtime-release-') as temporary:
        stage=Path(temporary)
        for archive in archives:
            checksum=archive.with_suffix(archive.suffix+'.sha256')
            expected=checksum.read_text(encoding='utf-8').split()[0]
            if hashlib.sha256(archive.read_bytes()).hexdigest()!=expected:raise ValueError('Runtime checksum mismatch')
            with tarfile.open(archive) as tar:
                files=[m for m in tar.getmembers() if m.name.endswith('/runtime.json') and m.isfile()]
                if len(files)!=1:raise ValueError('Runtime metadata missing or ambiguous')
                metadata=json.load(tar.extractfile(files[0]))
            if metadata.get('dirty', True) or metadata['source_commit']!=commit or metadata['abi']!=1:raise ValueError('Runtime provenance or ABI mismatch')
            if metadata['target'] in targets:raise ValueError('Duplicate runtime target')
            targets.add(metadata['target']);versions.add(metadata['version'])
            shutil.copyfile(archive,stage/archive.name);shutil.copyfile(checksum,stage/checksum.name)
        if len(versions)!=1:raise ValueError('Runtime versions differ')
        version=versions.pop()
        if not re.fullmatch(r'\d+\.\d+\.\d+',version):raise ValueError('Invalid runtime version')
        tag=f'sdk-runtime-v{version}-{commit[:12]}'
        api=GitHub(token)
        ref=api.request('GET',f'/repos/{repository}/git/ref/tags/{tag}')
        if ref is None:
            api.request('POST',f'/repos/{repository}/git/refs',{'ref':'refs/tags/'+tag,'sha':commit})
        elif ref['object']['sha']!=commit:raise ValueError('Runtime release tag targets another commit')
        api.release(repository,tag,stage,commit,body=f'Native adapter ABI v1 built from {commit}. Select the archive matching your OS/architecture; verify its adjacent SHA-256 file. Each archive contains runtime.json and the Node bridge.')

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts',type=Path,required=True)
    args=parser.parse_args()
    token_path=Path(os.environ['SDK_GITHUB_TOKEN_FILE'])
    publish_runtime(args.artifacts,os.environ['GITHUB_REPOSITORY'],os.environ['GITHUB_SHA'],token_path.read_text().strip())
