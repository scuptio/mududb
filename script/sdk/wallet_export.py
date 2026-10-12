"""Export canonical Wallet sources and bilingual usage guides."""
from pathlib import Path
import shutil
ROOT=Path(__file__).resolve().parents[2]
NAMES={'as':'AssemblyScript','py':'Python','cs':'C#','go':'Go','c':'C','rs':'Rust'}
COMMANDS={'py':'python adapter/main.py','cs':'dotnet run --project adapter/WalletAdapter.csproj --','go':'python adapter/run.py','c':'python adapter/run.py','as':'node adapter/main.mjs','rs':'cargo run --manifest-path adapter/Cargo.toml --'}
PREREQUISITES={'py':'Python 3.11+; MPK: componentize-py 0.25.1', 'cs':'.NET SDK 8 and runtime 8; MPK: Linux x64, .NET SDK 10 and the pinned Componentize.DotNet packages', 'go':'Go 1.24+ and a C compiler (CGO_ENABLED=1); MPK: compatible TinyGo with wasip2 support','c':'Clang; MPK: wasi-sdk 29 (set WASI_SDK)','as':'Node.js 22 and npm; MPK: AssemblyScript from the SDK lockfile','rs':'Rust from the main project rust-toolchain.toml; MPK: wasm32-wasip2 target'}

def export_wallet(lang, repo):
    dest=repo/'examples/wallet'
    shutil.copytree(ROOT/'script/sdk/examples'/lang,dest,ignore=shutil.ignore_patterns('__pycache__','target','bin','obj'),dirs_exist_ok=True)
    shutil.copyfile(ROOT/'script/sdk/wallet_build.py',dest/'build.py')
    command=COMMANDS[lang]
    entry={'py':'from mududb.adapter import use_adapter; use_adapter()',
           'cs':'mududb.sys.NativeAdapter.Use();',
           'rs':'mududb::adapter::use_adapter(std::env::var("MUDU_ADAPTER_LIBRARY")?)?;',
           'go':'client, err := adapter.Open("") // import github.com/mududb/mududb-go/adapter',
           'c':'#include "mududb/adapter/native.h" // mudu_native_load, call, release, unload',
           'as':'import { useAdapter } from "./adapter/index.mjs"; const call = useAdapter();'}[lang]
    import json
    metadata=json.loads((repo/'sdk-source.json').read_text())
    commit=metadata['source_commit']
    runtime_version=metadata.get('adapter_runtime_version', '0.1.0')
    release=f'https://github.com/scuptio/mududb/releases/tag/sdk-runtime-v{runtime_version}-{commit[:12]}'
    for chinese in (False,True):
        filename='README.zh-CN.md' if chinese else 'README.md'
        if chinese:
            text=f'''# {NAMES[lang]} Wallet：两种运行模式

- **MPK 模式**：业务编译为 Wasm 组件，在 Mudud 内运行。
- **Adapter 模式**：业务在本地运行，通过进程内原生库访问 SQLite、PostgreSQL、MySQL 或 Mudud。AssemblyScript 使用 Node 宿主运行 Wasm。

## 前置条件

{PREREQUISITES[lang]}。MPK 还需要 Python 3.11+、`mgen`、`mtp`、`mpm`、`mpm-build`、`wasm-tools`。运行时 Release 仅在手动启用发布后存在。这些 Mudu 工具需与 SDK 源版本匹配；在主仓库执行 `python script/build/install_binaries.py` 安装后加入 PATH。

## MPK

在本目录执行 `mpm build`，产物为 `target/wallet-{lang}.mpk`。启动 `mudud`，使用以下命令安装和调用：

```sh
mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wallet-{lang}.mpk
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet-{lang} --module wallet_{lang} --proc balance --json '{{"user_id":1001}}'
```

调用 balance 前先调用 create_user、deposit；过程参数见 `mpk` 业务源文件。MPK 使用宿主会话，不加载 adapter 动态库。

## Adapter

从 [匹配版本的运行时 Release]({release}) 下载并解压与操作系统、CPU 架构匹配的独立运行时附件，设置 `MUDU_ADAPTER_LIBRARY` 为其中动态库的绝对路径。保留同目录的 `mudu_adapter.node`（AS 使用）。设置 `MUDU_CONNECTION` 后再启动应用：

| 后端 | 连接示例 |
|---|---|
| SQLite | `sqlite://wallet.db` |
| PostgreSQL | `postgres://user:password@localhost:5432/wallet` |
| MySQL | `mysql://user:password@localhost:3306/wallet` |
| Mudud | `mudud://127.0.0.1:9527/wallet-{lang}?http_addr=127.0.0.1:8300` |

Bash 用 `export NAME=value`，PowerShell 用 `$env:NAME="value"`。为每次验证创建独立数据库；初始化不删除已有数据，重复初始化或重复账户会报错。PostgreSQL/MySQL 先创建专用数据库。Mudud 需先准备同名应用与表，连接字符串不会自动创建应用。本例可按上文执行 `mpm build` 和 `mcli app-install`，仅用 MPK 初始化应用及 schema，然后跳过下面的 `init`，直接运行 `scenario`。此时业务逻辑仍在本地通过 adapter 执行，不调用已部署的过程。两种模式分别使用全新的数据库或 Mudud 数据目录，避免重复账户。

```sh
{command} init
{command} scenario
{command} balance 1001
```

scenario 创建 Alice/Bob，存入 1000、取出 100、转账 250，预期余额为 650/250，并验证余额不足的拒绝行为。还支持 `create ID NAME EMAIL`、`deposit ID AMOUNT`、`withdraw ID AMOUNT`、`transfer FROM TO AMOUNT`。

业务示例展示接口调用；多语句事务语义随后端变化，不能把余额预检查当作并发安全或转账原子性保证。Mock SQLite demo 与本示例的真实 adapter 不同。

## 排错

缺少动态库：检查 MUDU_ADAPTER_LIBRARY；加载失败：核对系统、架构及系统依赖；ABI 不兼容：使用 ABI v1 的匹配运行时；缺工具：安装本页前置条件。不要在日志中输出含密码的连接字符串。
'''
        else:
            text=f'''# {NAMES[lang]} Wallet: MPK and native adapter

Prerequisites: {PREREQUISITES[lang]}. MPK builds additionally require Python 3.11+, matching `mgen`, `mtp`, `mpm`, `mpm-build`, and `wasm-tools` on PATH. Install Mudu tools from the matching main checkout with `python script/build/install_binaries.py`.

## MPK mode

Run `mpm build` here. Install `target/wallet-{lang}.mpk` into a running `mudud` using `mcli app-install --mpk target/wallet-{lang}.mpk`. Invoke `create_user`, `deposit`, `withdraw`, `transfer_funds`, and `balance` using `mcli app-invoke --app wallet-{lang} --module wallet_{lang} --proc ... --json ...`. See the signatures in `mpk/`. Business code runs inside Mudud; no native adapter library is loaded by the guest.

## Adapter mode

Business code runs locally and loads the real adapter runtime, not a mock. AS runs its business Wasm in a Node host. Download the [matching runtime Release]({release}) and extract the runtime matching your OS and architecture. Releases exist only after an explicitly enabled publish run. Set `MUDU_ADAPTER_LIBRARY` to the absolute library path (`.so`, `.dylib`, or `.dll`); retain the adjacent Node addon for AS. Set `MUDU_CONNECTION` before starting the process:

- SQLite: `sqlite://wallet.db`
- PostgreSQL: `postgres://user:password@localhost:5432/wallet`
- MySQL: `mysql://user:password@localhost:3306/wallet`
- Mudud: `mudud://127.0.0.1:9527/wallet-{lang}?http_addr=127.0.0.1:8300`

Use `export NAME=value` in Bash or `$env:NAME="value"` in PowerShell. Create a dedicated empty database first. Mudud additionally needs the named application and its schema; the connection URI does not create an application. For this example, run `mpm build` and `mcli app-install` as above to bootstrap the application/schema, then skip `init` below and run `scenario`. Business operations still execute locally through the adapter, without invoking deployed procedures. Use separate fresh databases or Mudud data directories for the two modes to avoid duplicate accounts.

```sh
{command} init
{command} scenario
{command} balance 1001
```

The scenario creates Alice/Bob, deposits 1000, withdraws 100, transfers 250, checks balances 650/250 and rejects insufficient funds. No existing data is deleted: duplicate initialization/accounts fail. Interactive commands: `create ID NAME EMAIL`, `deposit ID AMOUNT`, `withdraw ID AMOUNT`, `transfer FROM TO AMOUNT`, `balance ID`.

Backend transaction semantics differ. This interface example's balance checks alone do not guarantee concurrent transfer atomicity. Guest procedures use host sessions; the adapter runs local application code; the Mudud bootstrap above only prepares its application/schema.

## Troubleshooting

Check MUDU_ADAPTER_LIBRARY, OS/CPU compatibility and native dependencies for load errors; ABI v1 is required. Install the prerequisites above when a build tool is missing. Do not log database credentials. See [中文说明](README.zh-CN.md).
'''
        prefix = f'mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet-{lang} --module wallet_{lang}'
        calls = [('create_user', {'user_id':1001,'name':'Alice','email':'alice@example.com'}),
                 ('create_user', {'user_id':1002,'name':'Bob','email':'bob@example.com'}),
                 ('deposit', {'user_id':1001,'amount':1000}),
                 ('withdraw', {'user_id':1001,'amount':100}),
                 ('transfer_funds', {'from_user_id':1001,'to_user_id':1002,'amount':250}),
                 ('balance', {'user_id':1001}), ('balance', {'user_id':1002})]
        text += '\n## MPK invocation sequence / 完整调用顺序\n\n```sh\n'
        for proc, params in calls:
            text += prefix + ' --proc ' + proc + " --json '" + json.dumps(params) + "'\n"
        text += '```\n\nExpected final balances / 预期最终余额: Alice 650, Bob 250.\n'
        text += '\n## SDK adapter entry / SDK 接入入口\n\n```text\n' + entry + '\n```\n'
        if lang == 'cs':
            text += '\nC# MPK uses the existing WASI-compatible lightweight codec bridge; the native adapter example uses the distributed SDK facade. Both share `mpk/src/Procedures.cs`.\n'
        text = '<!-- Generated by script/sdk/wallet_export.py. Do not edit manually. -->\n' + text
        (dest/filename).write_text(text,encoding='utf-8')
        root=repo/filename
        if chinese:
            root.write_text(f'# mududb-{lang}\n\n参见 [Wallet 双模式使用指南](examples/wallet/README.zh-CN.md) 和 [API 文档](API.md)。原生 adapter 运行时独立分发，SDK 源码不包含动态库。\n',encoding='utf-8')
        else:
            s=root.read_text();s=s.replace('Guest syscall APIs require a compatible MuduDB host; this is not a general remote database client. Only explicitly documented mock backends run without that host.', 'Guest syscall APIs require a compatible MuduDB host. Native applications may explicitly select the separately distributed adapter runtime; mock backends remain separate.')
            root.write_text(s+'\nSee the [Wallet dual-mode guide](examples/wallet/README.md) and [中文说明](README.zh-CN.md).\n',encoding='utf-8')
