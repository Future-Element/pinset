# 集成与分发

Pinset 的集成始终选择精确 Pinset release 并验证下载归档，不使用未经验证的 `latest` 跳转。

## GitHub Actions

仓库根目录包含支持 Linux x64/ARM64、Windows x64 与 macOS ARM64 的 composite action。它会用 `SHA256SUMS` 验证 release 归档，把两个 Pinset 二进制加入 `PATH`，并默认安装项目锁定的运行时。

```yaml
steps:
  - uses: actions/checkout@v4
  - uses: Future-Element/pinset@v1.9.0
    with:
      version: 1.9.0
      install: "true"
  - run: pinset lock audit
  - run: pinset exec -- node --version
```

任务只需要 Pinset CLI 时设置 `install: "false"`；`working-directory` 用于指定执行 `pinset install --locked` 的目录。

## Renovate

把 [`integrations/renovate/pinset.json5`](../integrations/renovate/pinset.json5) 合并进仓库的 Renovate 配置，然后在每个需要管理的 TOML key 紧邻上一行添加注解：

```toml
[tools]
# renovate: datasource=node-version depName=node
node = "24.0.0"
# renovate: datasource=npm depName=pnpm
pnpm = "11.0.0"
```

显式 datasource 是必要边界：不同 Pinset Provider 消费不同上游版本系统，统一猜测 datasource 会产生错误更新。Renovate 修改 `pinset.toml` 后，应在更新工作流中运行 `pinset update` 来刷新并验证 `pinset.lock`。

## VS Code Schema

[`schemas/pinset.schema.json`](../schemas/pinset.schema.json) 与 [`schemas/pinset-lock.schema.json`](../schemas/pinset-lock.schema.json) 描述 schema 3 配置和锁文件。它们是 JSON Schema 文档，但被描述的文件使用 TOML 语法。VS Code 内置 JSON schema 关联只作用于 JSON 文件，因此需要安装支持 JSON Schema 的 TOML 语言扩展，并关联：

- `**/pinset.toml` → `https://raw.githubusercontent.com/Future-Element/pinset/v1.9.0/schemas/pinset.schema.json`
- `**/pinset.lock` → `https://raw.githubusercontent.com/Future-Element/pinset/v1.9.0/schemas/pinset-lock.schema.json`

使用固定 tag 的 URL 可保证补全行为可复现。`pinset.lock` 仍是生成状态，应由 Pinset 更新而不是手工编辑。

## Dev Container

把 [`examples/devcontainer/.devcontainer`](../examples/devcontainer/.devcontainer) 复制到 Pinset 项目。Dockerfile 会下载适配架构的 v1.9.0 归档、验证 `SHA256SUMS` 并安装 `pinset` 与 `pinset-shim`；工作区挂载后，`postCreateCommand` 再执行 `pinset install --locked`。

## Winget、Scoop 与 Homebrew

每个 GitHub Release 都包含：

- `pinset-winget.yaml`
- `pinset-scoop.json`
- `pinset.rb`

release 工作流会根据同一批已发布归档生成它们，把 manifest 哈希追加进 `SHA256SUMS`，并创建 GitHub artifact attestation。它们可以直接使用，也可以作为向上游 catalog 提交的来源。把变更发布到社区所有的包索引，属于生成 Pinset 官方 manifest 之外的独立维护者操作。
