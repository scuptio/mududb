"""Exercise examples from a disposable, independently exported SDK tree."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

def validate_wallet(lang, repo, artifacts, library, mode='both', connection=None, initialize=True):
    with tempfile.TemporaryDirectory(prefix=f'wallet-{lang}-') as temporary:
        temporary=Path(temporary)
        work=temporary/'sdk';shutil.copytree(repo,work,ignore=shutil.ignore_patterns('target','build','bin','obj','node_modules','__pycache__'))
        wallet=work/'examples/wallet'
        env=os.environ.copy()
        env.update(CARGO_BUILD_JOBS='4', RUST_TEST_THREADS='4', CARGO_PROFILE_DEV_DEBUG='1')
        env.pop('CARGO_TARGET_DIR',None)
        env.pop('PYTHONPATH',None)
        env['MUDU_ADAPTER_LIBRARY']=str(library)
        env['MUDU_CONNECTION']=connection or 'sqlite://'+str(temporary/'wallet.db')
        env['GOWORK']='off'
        if mode in ('adapter','both'):
            if lang=='as':
                from tooling import npm_command
                subprocess.run([*npm_command(shutil.which('node'),shutil.which('npm')),'ci','--ignore-scripts'],cwd=work,env=env,check=True)
            command={
                'py':[sys.executable,'adapter/main.py'],
                'cs':['dotnet','run','--project','adapter/WalletAdapter.csproj','--'],
                'go':[sys.executable,'adapter/run.py'],
                'c':[sys.executable,'adapter/run.py'],
                'as':['node','adapter/main.mjs'],
                'rs':['cargo','run','--manifest-path','adapter/Cargo.toml','--'],
            }[lang]
            for operation in (('init','scenario') if initialize else ('scenario',)):
                subprocess.run([*command,operation],cwd=wallet,env=env,check=True)
            failed = subprocess.run([*command, 'withdraw', '1001', '10000'], cwd=wallet, env=env, capture_output=True, text=True)
            if failed.returncode == 0 or 'insufficient funds' not in failed.stdout + failed.stderr:
                raise RuntimeError(f'{lang}: expected insufficient-funds rejection')
            result = subprocess.run([*command, 'balance', '1001'], cwd=wallet, env=env, capture_output=True, text=True, check=True)
            if result.stdout.strip().splitlines()[-1] != '650':
                raise RuntimeError(f'{lang}: failed withdrawal changed balance')
        if mode in ('mpk','both'):
            subprocess.run([sys.executable,'build.py',lang],cwd=wallet,env=env,check=True)
            destination=artifacts;destination.mkdir(exist_ok=True)
            shutil.copyfile(wallet/'target'/f'wallet-{lang}.mpk',destination/f'wallet-{lang}.mpk')
            from mpk_smoke import verify
            verify(destination/f'wallet-{lang}.mpk', lang)
