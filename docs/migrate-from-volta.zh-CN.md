# 从 Volta 迁移项目到 Pinset

把项目 `package.json` 中声明的 Node 和 pnpm 版本导入 Pinset，审阅精确锁，再核对命令路径后切换 shell。本文也说明 Pinset 无法独立保留的 npm 固定版本边界。

[English](migrate-from-volta.md) · [命令参考](commands.zh-CN.md) · [基础概念](concepts.zh-CN.md)

Volta 维护者已在[官方仓库声明停止维护](https://github.com/volta-cli/volta#readme)，同时说明正常工作的安装无需紧急迁移。按项目自身需求安排切换；下面的步骤保留 Volta，方便对照和回退。

## 三类字段如何映射

在项目根目录执行导入。假设现有 `package.json` 包含：

```json
{
  "volta": {
    "node": "24.0.0",
    "npm": "11.4.2",
    "pnpm": "10.15.0"
  }
}
```

这些版本用于展示已有声明的迁移，不是新项目的版本推荐。

| Volta 声明 | Pinset 的处理 |
| --- | --- |
| `volta.node` | 导入 `.pinset/config.toml` 中的精确 Node 选择，并在 `.pinset/lock.toml` 记录产物。 |
| `volta.pnpm` | 导入精确 pnpm 选择和锁；项目还必须选择 Node，应把两者一起导入。 |
| `volta.npm` | 记入导入报告的 `unsupported`；Pinset 没有独立 npm Provider，也没有 `[tools].npm` 选择。 |
| `volta.extends` | 忽略；Pinset 不读取所引用的 package 文件，也不导入其中继承的版本。 |

Pinset 的 `npm`、`npx` 使用所选 Node 发行包附带的版本，因此实际 npm 可能与 `volta.npm` 不同；`pinset check` 通过也不代表原 npm 固定版本得到保留。如果项目必须独立固定为该 npm 版本，应先解决这个要求，再切换到 Pinset。`pinset use npm@11.4.2` 不是受支持的替代操作。

Volta 的 [pnpm 支持](https://docs.volta.sh/advanced/pnpm) 需要启用实验性开关。项目实际存在的 `volta.pnpm` 声明可以导入；仅安装在 Volta 机器配置中的 pnpm 不在导入范围内。Volta 的全局默认版本、已安装的包命令、hooks 和下载缓存也不会迁移。

## 保留当前可工作的环境

先保留 Volta 安装、`package.json` 和其他版本文件。备份已有的 `.pinset/` 目录、`.gitignore`、shell 启动配置以及 PATH 顺序。导入保留来源文件，但新建 Pinset 项目时也会补充本地状态的忽略规则。

改变命令优先级前，记录当前 Volta 环境中的版本和路径：

```sh
volta list
volta which node
volta which npm
node --version
npm --version
```

项目使用 pnpm 时，再记录 `volta which pnpm` 和 `pnpm --version`。保存当前 shell 的 PATH，供后续回退使用：

```bash
volta_migration_path=$PATH
```

PowerShell 使用：

```powershell
$voltaMigrationPath = $env:PATH
```

按官网安装说明装好 Pinset CLI，让 `pinset` 命令可以执行；先不要把 `pinset self shell` 输出写入 shell 启动配置，等下面的对照通过后再切换。`pinset self info` 显示 CLI 路径和 Pinset home；受管命令目录可从 `pinset self shell` 输出核对。

## 审阅并导入版本声明

逐条执行命令，遇到错误就停下处理。无需先运行 `pinset init`：

```sh
pinset import --plan
```

检查计划中的 `imports`、`retained`、`conflicts`、`unresolved` 和 `unsupported`；脚本可用 `pinset import --plan --json` 读取同样的信息。计划可能在跳过部分条目后仍以 `0` 退出，因此只看退出码不能判断是否完整迁移。

确认 Node 和 pnpm 的精确版本符合预期。上面的示例中，npm 会出现在 `unsupported`；只有对照实际 npm 版本和项目要求后，才能接受改用 Node 附带的 npm。其他跳过项也应逐一处理后再继续。

导入还会读取根目录的 `engines`、`packageManager`、`.nvmrc`、`.node-version` 等受支持版本声明。同一工具的兼容声明共同约束结果，不存在“Volta 固定版本自动覆盖其他来源”的规则。已有 Pinset 选择会保留，不会被替换。先统一冲突的声明，或用 `pinset use` 明确改变已有选择，再重新审阅迁移计划。

使用 `volta.extends` 的项目，应先逐个确认相关包的实际生效版本。导入不会追踪这些引用，也不会递归发现包清单。为每个计划使用的 Pinset 项目明确选择实际版本，边界见[项目与全局范围](concepts.zh-CN.md#项目与全局范围)。冲突列表为空，不代表 Volta 继承配置已经迁移。

计划完整、npm 边界也符合项目要求后，再执行：

```sh
pinset import
pinset install
pinset check
```

`import` 显示计划，确认后写入配置和精确锁，不安装工具；确认前再次检查本次显示的计划。`install` 按锁还原产物；`check` 在所选工具的请求检查失败时以 `1` 退出。把 `.pinset/config.toml` 和 `.pinset/lock.toml` 一起纳入版本控制。

## 先核对，再切换 PATH

保留 Volta 可用，先显式检查 Pinset 路由并执行它选择的工具：

```sh
pinset which node --explain
pinset which npm --explain
pinset which pnpm --explain
pinset exec -- node --version
pinset exec -- npm --version
pinset exec -- pnpm --version
```

项目未选择 pnpm 时，跳过对应命令。与之前记录的版本对照，再用 `pinset exec -- <命令>` 执行项目日常检查。Pinset 的完整性检查不能代替这些项目检查，也不能代替明确的 npm 版本对照。

结果符合项目要求后，再为当前 shell 启用受管命令目录。Bash 使用：

```bash
pinset_shell=$(pinset self shell bash) && eval "$pinset_shell"
hash -r
command -v node npm pnpm
```

PowerShell 使用：

```powershell
$pinsetShell = pinset self shell powershell
if ($LASTEXITCODE -ne 0) { throw "Pinset 未能生成 shell 配置。" }
$pinsetShell | Out-String | Invoke-Expression
Get-Command node, npm, pnpm
```

不用 pnpm 时，同样从路径查询中去掉它。确认直接执行的 `node`、`npm` 和适用时的 `pnpm` 已经经过 Pinset，版本也符合预期；路径仍不一致时，检查更早生效的别名或函数。之后再把对应 shell 片段保存到启动配置。`self shell` 只输出片段，不会修改启动文件。

团队和 CI 完成切换、回退期结束前，保留原 Volta 字段。Pinset 正常执行时读取自己的配置和锁，不会把 Volta 设置当作运行时后备来源。CI 接入见 [setup-pinset 指南](ci.zh-CN.md)。

## 如何回退

在保存 PATH 的同一个 shell 中恢复：

```bash
export PATH="$volta_migration_path"
hash -r
```

PowerShell 使用：

```powershell
$env:PATH = $voltaMigrationPath
```

如果已把 Pinset 片段写入 shell 启动文件，恢复该文件原来的 PATH 顺序，再打开新 shell。用 `volta which node`、shell 命令路径查询和已记录的版本输出，确认命令回到原来的环境。

只恢复本次迁移改动过的项目文件，包括修改过的版本声明、`.pinset/` 状态和 `.gitignore` 规则。原来就有 `.pinset/` 时恢复其旧内容，不要整个删除；本次新建该目录时，也先确认没有其他需要保留的工作，再移除这部分新项目状态。Pinset 的共享安装和缓存目录可以保留，回到 Volta 不需要删除它们。

项目、协作者和 CI 接受替代后的行为（包括 npm）之前，不要卸载 Volta 或移除它的版本声明。
