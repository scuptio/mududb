"""Install and invoke one SDK Wallet MPK against a disposable real Mudud server."""
import argparse
import base64
from contextlib import contextmanager
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0))
        return sock.getsockname()[1]

@contextmanager
def server():
    with tempfile.TemporaryDirectory(prefix='wallet-mudud-') as temp:
        root=Path(temp)
        ports=set()
        while len(ports)<3:ports.add(free_port())
        http,tcp,pg=sorted(ports)
        config=root/'mudud.toml'
        config.write_text(f'''mpk_path = {json.dumps(str(root/'mpk'))}
db_path = {json.dumps(str(root/'data'))}
listen_ip = "127.0.0.1"
http_listen_port = {http}
pg_listen_port = {pg}
tcp_listen_port = {tcp}
http_worker_threads = 1
worker_threads = 2
server_mode = "Tokio"
enable_async = true
component_target = "p2"
''',encoding='utf-8')
        (root/'mpk').mkdir();(root/'data').mkdir()
        with (root/'server.log').open('w+') as log:
            process=subprocess.Popen(['mudud','serve','--cfg',str(config)],cwd=root,stdout=log,stderr=subprocess.STDOUT)
            base=f'http://127.0.0.1:{http}'
            def request(path,body=None):
                data=json.dumps(body).encode() if body is not None else None
                req=urllib.request.Request(base+path,data=data,headers={'Content-Type':'application/json'})
                try:
                    with urllib.request.urlopen(req,timeout=120) as response:
                        value = json.load(response)
                        if isinstance(value, dict) and 'ok' in value and 'data' in value:
                            if not value['ok']: raise RuntimeError(f'{path}: {value}')
                            return value['data']
                        return value
                except urllib.error.HTTPError as error:
                    raise RuntimeError(f'{path}: HTTP {error.code}: {error.read().decode()}') from None
            try:
                deadline=time.monotonic()+30
                while True:
                    if process.poll() is not None:
                        log.seek(0);raise RuntimeError('Mudud exited: '+log.read()[-5000:])
                    try:request('/mudu/app/list');break
                    except (OSError,RuntimeError):
                        if time.monotonic()>deadline:raise RuntimeError('Timed out starting Mudud') from None
                        time.sleep(.1)
                yield request, f'127.0.0.1:{tcp}',f'127.0.0.1:{http}'
            finally:
                process.terminate()
                try:process.wait(timeout=15)
                except subprocess.TimeoutExpired:process.kill();process.wait()


def verify(mpk,lang):
    with server() as (request,_,_):
        request('/mudu/app/install',{'mpk_base64':base64.b64encode(Path(mpk).read_bytes()).decode()})
        def invoke(proc,expected,**params):
            result=request(f'/mudu/app/invoke/wallet-{lang}/wallet_{lang}/{proc}',params)
            if result.get('return_list')!=[[0,[9,expected]]]:raise AssertionError((proc,result,expected))
        invoke('create_user',1001,user_id=1001,name='Alice',email='alice@example.com')
        invoke('create_user',1002,user_id=1002,name='Bob',email='bob@example.com')
        invoke('deposit',1000,user_id=1001,amount=1000)
        invoke('withdraw',900,user_id=1001,amount=100)
        invoke('transfer_funds',650,from_user_id=1001,to_user_id=1002,amount=250)
        invoke('balance',650,user_id=1001)
        invoke('balance',250,user_id=1002)
        try:
            request(f'/mudu/app/invoke/wallet-{lang}/wallet_{lang}/withdraw',{'user_id':1001,'amount':10000})
        except RuntimeError as error:
            if 'insufficient funds' not in str(error) and '50017' not in str(error):raise
        else:raise AssertionError('overdraft was accepted')
        invoke('balance',650,user_id=1001)
        print(f'{lang}: real Mudud MPK scenario passed')

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('language');parser.add_argument('mpk',type=Path)
    args=parser.parse_args()
    if args.language == 'all':
        for lang in ('as', 'py', 'cs', 'go', 'c', 'rs'):
            verify(args.mpk/lang/'examples/wallet/target'/f'wallet-{lang}.mpk',lang)
    else: verify(args.mpk,args.language)
