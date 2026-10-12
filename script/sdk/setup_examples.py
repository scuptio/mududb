"""Install the Linux guest tools used by the manually enabled SDK example job."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import urllib.request

def main():
    if sys.platform != 'linux':
        raise RuntimeError('Full MPK toolchain setup is currently Linux x64 only')
    selected=os.environ.get('SDK_LANGS','as,py,cs,go,c,rs').split(',')
    home=Path(os.environ.get('RUNNER_TEMP', '/tmp'))/'sdk-guest-tools';home.mkdir(exist_ok=True)
    subprocess.run(['cargo','install','wasm-tools','--version','1.252.0','--locked'],check=True)
    for binary,package in [('mgen','mudu_gen'),('mtp','mudu_transpiler'),('mpm','mpm_build'),('mpm-build','mpm_build')]:
        subprocess.run(['cargo','install','--path',f'crates/tools/{package}','--bin',binary,'--locked','--force'],check=True)
    subprocess.run(['cargo','install','--path','crates/db-kernel/mudud','--bin','mudud','--locked','--force'],check=True)
    if 'py' in selected:
        subprocess.run([sys.executable,'-m','pip','install','componentize-py==0.25.1'],check=True)
    downloads=[]
    if 'go' in selected:
        downloads.append(('tinygo.tar.gz','https://github.com/tinygo-org/tinygo/releases/download/v0.42.0/tinygo0.42.0.linux-amd64.tar.gz'))
    if 'c' in selected:
        downloads.append(('wasi.tar.gz','https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-29/wasi-sdk-29.0-x86_64-linux.tar.gz'))
    for name,url in downloads:
        archive=home/name;urllib.request.urlretrieve(url,archive)
        with tarfile.open(archive) as tar:tar.extractall(home,filter='data')
    if 'GITHUB_PATH' in os.environ and 'go' in selected:
        with open(os.environ['GITHUB_PATH'],'a',encoding='utf-8') as stream:stream.write(str(home/'tinygo/bin')+'\n')
    if 'GITHUB_ENV' in os.environ and 'c' in selected:
        wasi=next(p for p in home.glob('wasi-sdk-*') if p.is_dir())
        with open(os.environ['GITHUB_ENV'],'a',encoding='utf-8') as stream:stream.write('WASI_SDK='+str(wasi)+'\n')
if __name__=='__main__':main()
