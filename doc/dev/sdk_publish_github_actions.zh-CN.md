# 使用 GitHub Actions 发布多语言 SDK

本文对应主仓库 `scuptio/mududb` 的 [SDK Publish 工作流](../../.github/workflows/sdk-publish.yaml)。
工作流仅支持手动触发，不会在 push、Pull Request 或定时任务中自动发布。

发布目标是各语言的独立 GitHub 仓库、版本标签和 GitHub Releases。
此工作流不向 npm、PyPI、NuGet 或 crates.io 注册中心发布。
本地构建和 SDK 使用说明见 [SDK distributions](sdk_distribution.md)。

首次发布按第 1–5 节完成仓库、Token 和版本准备；后续发布通常从第 4 节开始。失败处理见第 7 节，修改发布实现见第 8 节。

| 名称 | 本项目中的位置 | 用途 |
|---|---|---|
| 源仓库 | `scuptio/mududb` | 运行工作流，保存 `SDK_PUBLISH_TOKEN` Secret |
| SDK 目标仓库 | `mududb/mududb-<语言>` | 接收 SDK 提交、标签和 Release |
| Token 的 Resource owner | `mududb` | 授权写入 SDK 目标仓库 |

## 1. 确认发布目标

目前目标地址固定在构建和发布脚本中，不能通过 Actions 参数修改：

| 语言 | `languages` 值 | 目标仓库 |
|---|---|---|
| AssemblyScript | `as` | `mududb/mududb-as` |
| Python | `py` | `mududb/mududb-py` |
| C# | `cs` | `mududb/mududb-cs` |
| Go | `go` | `mududb/mududb-go` |
| C | `c` | `mududb/mududb-c` |
| Rust | `rs` | `mududb/mududb-rs` |

首次发布可以手动创建这些公开空仓库，不要初始化 README、LICENSE 或 `.gitignore`。
也可以通过 `create_repos=true` 自动创建，所需权限见下一节。
已有目标仓库必须公开、未归档，并允许发布账号写入。

## 2. 创建发布 Token

使用有权访问 `mududb` 组织及目标仓库的个人账号，打开：

[创建 Fine-grained personal access token](https://github.com/settings/personal-access-tokens/new)

也可以从右上角个人头像进入 **Settings → Developer settings → Personal access tokens → Fine-grained tokens → Generate new token**。
组织设置中的 Token 页面用于审核和管理，创建入口在个人设置中。

填写：

| 字段 | 设置 |
|---|---|
| Token name | 例如 `mududb-sdk-publisher` |
| Expiration | 按维护周期设置，到期前更新 Secret |
| Resource owner | `mududb`，即目标 SDK 仓库所属组织 |
| Repository access | 按下方两种情况选择 |

### 已经创建好目标仓库

选择 **Only select repositories**，选中需要发布的 SDK 仓库。
在 **Repository permissions** 中配置：

| 权限 | 级别 | 用途 |
|---|---|---|
| Contents | Read and write | 推送提交和标签、创建 Release、上传附件 |
| Metadata | Read-only，自动添加 | 读取仓库信息 |

运行时设置 `create_repos=false`。

### 需要工作流自动创建仓库

选择 **All repositories**，使 Token 可以访问之后创建的仓库。
除上述权限外，添加 **Repository permissions → Administration → Read and write**。
运行时设置 `publish=true`、`create_repos=true`。
账号本身也必须被组织策略允许创建公开仓库；Token 不会赋予账号原本没有的权限。

SDK 发布 Token 不需要 Actions、Workflows 或 Packages 权限。
如果 Token 显示 **Pending**，需组织管理员批准后才能写入仓库。
如果 Resource owner 中没有 `mududb`，检查组织成员身份和组织的 fine-grained Token 策略。

参考：[GitHub Token 配置](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)、
[组织仓库创建权限](https://docs.github.com/en/rest/repos/repos#create-an-organization-repository)。

## 3. 将 Token 保存为源仓库 Secret

打开 **`scuptio/mududb` → Settings → Secrets and variables → Actions → Secrets → New repository secret**。

```text
Name: SDK_PUBLISH_TOKEN
Secret: 粘贴刚刚生成的 Token
```

点击 **Add secret**。Secret 放在运行工作流的源仓库，而不是六个 SDK 目标仓库中。
使用 Secrets，不要使用 Variables，也不要将 Token 写入代码或文档。

也可以通过 GitHub CLI 设置，在提示出现后粘贴 Token：

```bash
gh secret set SDK_PUBLISH_TOKEN --repo scuptio/mududb
```

工作流已经读取 `${{ secrets.SDK_PUBLISH_TOKEN }}`，无需修改 YAML。
可选的原生 adapter runtime Release 使用源仓库自身的 `GITHUB_TOKEN`，工作流已为该任务配置 `contents: write`。

参考：[GitHub Actions Secrets](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets)。

## 4. 设置 SDK 版本并推送源代码

版本号必须是稳定版本格式 `X.Y.Z`，例如 `0.1.1`。

| 语言 | 版本来源 |
|---|---|
| AS | `crates/sdk/bindings/assemblyscript/package.json` 的 `version` |
| Python | `crates/sdk/bindings/python/pyproject.toml` 的 `project.version` |
| C# | `crates/sdk/mudu_api/csharp/Mudu.Api.csproj` 的 `Version` |
| Rust | `crates/sdk/mudu_api/rust/Cargo.toml` 的 `package.version` |
| C、Go | `script/sdk/config.json` 对应语言的 `version` |

提交并推送需要发布的源代码、版本变更、工作流及其依赖脚本。
GitHub Actions 读取远端所选分支的提交，不会读取本地未提交内容。
工作流文件需要存在于默认分支，才能从 Actions 页面使用手动运行入口。

已有版本不能用于不同源提交或内容；即使 SDK 业务代码相同，源提交元数据变化也会影响一致性检查。

## 5. 手动运行工作流

打开 **源仓库 → Actions → SDK Publish → Run workflow**，选择源分支并填写参数：

| 参数 | 默认值 | 含义 |
|---|---|---|
| `languages` | `as,py,cs,go,c,rs` | 选择发布语言；也可填 `py,go`，逗号之间不要加空格 |
| `publish` | `false` | 是否真正推送 SDK 仓库并创建 Releases |
| `create_repos` | `false` | 是否创建缺失的公开 SDK 仓库，仅发布时生效 |
| `with_adapter_runtime` | `false` | 构建各平台原生 adapter runtime；开启发布时上传到源仓库的独立 Release |
| `with_examples` | `false` | 验证各平台原生 Wallet 示例及 Linux MPK 示例，同时包含 runtime |

首次验证可保持 `publish=false`，只构建、验证和生成 Actions Artifacts。
正式发布时勾选 `publish`；只有需要自动建库时才勾选 `create_repos`。

也可以在命令行发布到已存在的仓库：

```bash
gh workflow run sdk-publish.yaml --repo scuptio/mududb \
  -f languages=as,py,cs,go,c,rs \
  -f publish=true -f create_repos=false
```

首次需要自动建库时改为 `-f create_repos=true`；只验证时改为 `-f publish=false`。
指定源分支可追加 `--ref <branch>`，验证 Wallet 示例可追加 `-f with_examples=true`。
每条命令都会启动一次新的运行；从验证切换到正式发布需要再次触发工作流。

参考：[手动运行工作流](https://docs.github.com/zh/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)。

## 6. 发布后会发生什么

工作流先在 Linux、macOS、Windows 上构建并验证所选 SDK。
三个平台全部成功后，发布任务使用 Linux 构建产物，不重新构建 SDK。

每个新版本在对应目标仓库中：

1. 在 `main` 上追加一条提交，保留已有历史。
2. 创建 `vX.Y.Z` 标签。
3. 创建 GitHub Release，上传安装包及相关附件，上传完成后公开 Release。

提交作者为 `MuduDB SDK Publisher`，提交信息类似：

```text
SDK v0.1.1 from <source_commit_sha>
```

| 情况 | 结果 |
|---|---|
| 新版本 | 新增提交、标签和 Release |
| 同版本且内容完全一致 | 不新增提交，检查并补齐 Release 附件 |
| 同版本但内容不同 | 报错，需升级版本 |
| 仓库缺失，`create_repos=false` | 报错停止 |
| 仓库缺失，`create_repos=true` | 有权限时创建公开仓库，再发布 |
| 已有仓库含未受脚本管理的内容或手动改动 | 报错，不覆盖 |

工作流不会强制推送或替换已有版本标签。
六个仓库的发布不是一个整体原子操作，可能部分成功、部分失败。
SDK 仓库发布与可选 runtime 发布也是独立任务，需要分别检查结果。

## 7. 查看结果与排查失败

在工作流运行页面下载构建产物：

- `sdk-distributions-Linux`
- `sdk-distributions-macOS`
- `sdk-distributions-Windows`

发布阶段生成报告后，还会上传 `sdk-publish-report`。
若在发布前置校验阶段就失败，可能没有报告，请查看任务日志。

| 问题 | 检查项 |
|---|---|
| 没有 Run workflow 按钮 | 默认分支是否包含工作流，账号是否有运行权限 |
| 没有 Generate new token 按钮 | 是否进入了组织管理页；使用个人 Token 创建链接 |
| Token 缺失或无效 | 源仓库 Secret 名称是否准确、Token 是否过期或仍待批准 |
| Repository missing or inaccessible | 目标仓库是否存在、Token 是否覆盖该仓库；404 也可能代表无权访问 |
| 建库失败 | 是否开启 `create_repos`、Token 是否有 Administration 写权限、组织是否允许建库 |
| Version already exists with different content | 升级对应 SDK 版本并提交，不复用旧版本号 |
| Remote contains unmanaged changes | 检查目标仓库手动修改；首次发布使用空仓库 |
| build 失败、publish 被跳过 | 先解决对应平台的构建或验证错误 |

部分发布失败后，优先在同一次运行中重跑失败的发布任务，复用原构建产物。
已匹配的标签和附件可被复用，缺失附件会继续上传；内容冲突不会被覆盖。
不要为了重试修改内容后继续使用同一个已发布版本号。

## 8. 维护与安全

### 目标仓库在哪个文件中定义

当前没有独立的目标仓库配置项；仓库名由 Python 脚本生成并校验。
其数据字段名是 **`repository`**，值的格式为 **`owner/repo`**，例如 `mududb/mududb-py`，不包含 `https://github.com/` 或 `.git`。

| 文件 | 位置或名称 | 当前行为 |
|---|---|---|
| [`script/sdk/sdk_build.py`](../../script/sdk/sdk_build.py) | `export()` 中的 `metadata['repository']` | 使用 `f'mududb/mududb-{lang}'`，写入生成的 `sdk-source.json` |
| [`script/sdk/sdk_build.py`](../../script/sdk/sdk_build.py) | 构建清单 `entries` 中的 `repository` | 使用 `f'mududb/{name}'`，其中 `name` 为 `mududb-<lang>`；写入 `manifest.json` 的 `sdks[]` |
| [`script/sdk/publish.py`](../../script/sdk/publish.py) | `load_manifest()` | 要求 `entry['repository']` 等于 `f'mududb/mududb-{lang}'` |
| [`script/sdk/publish.py`](../../script/sdk/publish.py) | `prepare()` | 使用 `entry['repository']` 拼接 Git remote：`https://github.com/<owner>/<repo>.git` |
| [`script/sdk/publish.py`](../../script/sdk/publish.py) | `GitHub.create()` | 建库接口固定为 `/orgs/mududb/repos`，仓库名取 `repository` 中 `/` 后面的部分 |

`script/sdk/config.json` 目前配置导出目录和 C/Go 版本，不读取 `repository` 字段。Actions 参数也不能改变目标地址。

### 如果需要修改目标仓库

目前需要修改实现，不能只改一个 JSON 字段：

1. 同步修改 `sdk_build.py` 中两处目标仓库元数据生成逻辑。
2. 同步修改 `publish.py` 的目标校验；变更组织时还需修改 `GitHub.create()` 的建库地址。
3. 更新生成 README 的安装命令、AS 包的 `repository` 地址等相关引用。
4. Go 还需同步更新 `go.mod` 的 module、相关 import，以及 `sdk_build.py` 中的模块路径校验。
5. 更新 Token 的 Resource owner 和目标仓库授权，重新构建后发布。

修改后重新构建产物；不要手改 `manifest.json` 或 `sdk-source.json`，发布前会验证其哈希和元数据。

### 凭据防护与自动扫描

- 根目录 `.gitignore` 忽略任意层级的 `.env`、`.env.*`、`.secrets/`、常见凭据目录、包管理器认证文件和私钥文件。
  `.env.example`、`.env.sample`、`.env.template` 可提交，但只能填写占位值。
  忽略规则不会移除已经被 Git 跟踪的文件。
- `script/sdk/sdk_build.py` 的 `CREDENTIAL_NAMES`、`CREDENTIAL_PATTERNS` 和 `ENV_TEMPLATES`
  定义导出路径规则。导出根目录内发现凭据路径会失败；私钥头也会被检测，改扩展名不能绕过。
  完整生成目录会在创建源归档前再次校验，发布时计算清单哈希也使用相同检查。
- [Secret Scan 工作流](../../.github/workflows/secret-scan.yaml) 在 push、PR 和手动运行时扫描检出的完整 Git 历史。
  SDK Publish 的构建任务依赖扫描通过，因此扫描失败时不会构建、上传或发布 SDK。
  要阻止含泄露的 PR 合并，还需在 GitHub 分支规则中将 `Scan secrets` 配置为必需检查。
- 扫描使用固定版本并校验 SHA-256 的 Gitleaks CLI，不需要额外 Token 或 Gitleaks Action 许可证。
  日志启用完整脱敏，不上传扫描报告；`.gitleaks.toml` 继承默认检测规则，不设置批量历史豁免。

安装工作流中固定的 Gitleaks 版本后，可以本地运行：

```bash
# 扫描所有本地 Git 引用可达的历史，包含之后已删除的内容。
bash script/ci/check_secrets.sh

# 单独扫描本地 SDK 输出目录的文件内容。
bash script/ci/check_secrets.sh target/sdk-dist
```

文件名拒绝规则无法识别所有普通源码中的密码，模式扫描也不能保证识别所有凭据。
扫描若发现真实密钥，应先撤销或轮换，再处理代码与历史；仅删除当前文件不足以消除历史泄露。

### 文件入口

- [工作流](../../.github/workflows/sdk-publish.yaml)
- [构建与导出脚本](../../script/sdk/sdk_build.py)
- [SDK 发布脚本](../../script/sdk/publish.py)
- [语言导出配置](../../script/sdk/config.json)
- [完整 SDK 分发文档](sdk_distribution.md)

本指南保存在主仓库中。当前导出逻辑不会将本指南复制到各语言 SDK 仓库；
这些仓库提供安装 README、API 文档和 Wallet 示例使用指南。
