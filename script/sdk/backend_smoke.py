"""Run all selected native Wallets against disposable PostgreSQL/MySQL containers."""
import argparse
from contextlib import contextmanager
import os
from pathlib import Path
import subprocess
import tempfile
import time
from wallet_validate import validate_wallet

@contextmanager
def database(backend):
    image,port=('postgres:16',5432) if backend=='postgres' else ('mysql:8.4',3306)
    options=['-e','POSTGRES_PASSWORD=sdk-test'] if backend=='postgres' else ['-e','MYSQL_ROOT_PASSWORD=sdk-test','-e','MYSQL_ROOT_HOST=%']
    container=subprocess.check_output(['docker','run','--rm','-d',*options,'-p',f'127.0.0.1::{port}',image],text=True).strip()
    try:
        address=subprocess.check_output(['docker','port',container,str(port)],text=True).strip()
        host_port=int(address.rsplit(':',1)[1])
        check=['pg_isready','-U','postgres'] if backend=='postgres' else ['mysqladmin','ping','-h','127.0.0.1','-uroot','-psdk-test','--silent']
        deadline=time.monotonic()+90
        while subprocess.run(['docker','exec',container,*check],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode:
            if time.monotonic()>deadline:raise RuntimeError(f'{backend} did not become ready')
            time.sleep(.5)
        def create(lang):
            name='sdk_wallet_'+lang
            args=['createdb','-U','postgres',name] if backend=='postgres' else ['mysql','-uroot','-psdk-test','-e',f'CREATE DATABASE {name}']
            subprocess.run(['docker','exec',container,*args],check=True,stdout=subprocess.DEVNULL)
            user='postgres' if backend=='postgres' else 'root'
            return f'{backend}://{user}:sdk-test@127.0.0.1:{host_port}/{name}'
        yield create
    finally:
        subprocess.run(['docker','stop',container],check=True,stdout=subprocess.DEVNULL)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dist',type=Path,required=True)
    parser.add_argument('--library',type=Path,required=True)
    parser.add_argument('--languages',default='as,py,cs,go,c,rs')
    parser.add_argument('--review-layout',action='store_true')
    parser.add_argument('--backends',default='postgres,mysql,mudud')
    args=parser.parse_args()
    from sdk_build import languages
    selected=languages(args.languages)
    backends=args.backends.split(',')
    if any(x not in ('postgres','mysql','mudud') for x in backends):raise ValueError('Invalid backend selection')
    for backend in (b for b in backends if b != 'mudud'):
        with database(backend) as create, tempfile.TemporaryDirectory() as temporary:
            for lang in selected:
                repo=args.dist/lang if args.review_layout else args.dist/f'mududb-{lang}'/'repo'
                validate_wallet(lang,repo,Path(temporary),args.library,'adapter',connection=create(lang))
                print(f'{backend}/{lang}: native Wallet passed',flush=True)
    from mpk_smoke import server
    import base64
    if 'mudud' not in backends:return
    for lang in selected:
        with server() as (request, tcp, http), tempfile.TemporaryDirectory() as temporary:
            repo=args.dist/lang if args.review_layout else args.dist/f'mududb-{lang}'/'repo'
            mpk=(repo/'examples/wallet/target'/f'wallet-{lang}.mpk') if args.review_layout else args.dist/f'mududb-{lang}'/'artifacts'/f'wallet-{lang}.mpk'
            request('/mudu/app/install', {'mpk_base64':base64.b64encode(mpk.read_bytes()).decode()})
            validate_wallet(lang,repo,Path(temporary),args.library,'adapter',connection=f'mudud://{tcp}/wallet-{lang}?http_addr={http}',initialize=False)
            print(f'mudud/{lang}: native Wallet passed',flush=True)
if __name__=='__main__':main()
