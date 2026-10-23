# 使用 setup-pinset 接入 CI

在 GitHub Actions 中安装 Pinset、按已提交的锁还原工具链，并让安装或校验失败直接阻止工作流通过。仓库根目录的官方 `setup-pinset` Action 从 v3.0.9 提供，以下示例固定 v3.1.0 发布标签；`version` 输入默认安装最新稳定 Pinset 3。

[English](ci.md) · [完整命令](commands.zh-CN.md)

```yaml
name: Build
on: [push, pull_request]
jobs:
  build:
    strategy:
      matrix:
        os: [ubuntu-24.04, macos-15, windows-2025]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: Future-Element/pinset@v3.1.0
        id: pinset
        # version 默认 latest；需要可复现 CI 时填写确切发布版本。
      - run: pinset exec -- node --version
```

运行前提交 `.pinset/config.toml` 和 `.pinset/lock.toml`，锁中须包含各 runner 平台的产物。Pinset 二进制支持 Linux x64/ARM64、macOS ARM64、Windows x64；不支持 macOS Intel 和 Windows ARM64 runner。Action 分别使用原生安装脚本，并将 CLI 目录和 `PINSET_HOME/v3/bin` 加入 PATH。

## 输入和输出

| 输入 | 默认值 | 含义 |
| --- | --- | --- |
| `version` | `latest` | 最新稳定 Pinset 3 版本，或确切的 `3.x.y`/`v3.x.y` 发布版本，也接受受支持的 `-rc.N` 版本。 |
| `install` | `true` | 还原项目工具并检查。设为 `false` 时只安装 Pinset，跳过项目缓存、还原和检查。 |
| `cache` | `true` | 缓存、还原工具安装目录。 |
| `skip-check` | `false` | 跳过安装后的检查；仍执行按锁安装及下载产物摘要校验。 |
| `working-directory` | `.` | 项目目录，或 Pinset 项目边界内的子目录。 |
| `token` | `${{ github.token }}` | 查询发布元数据时使用的 GitHub token。 |

| 输出 | 含义 |
| --- | --- |
| `version` | 解析后的确切 Pinset 版本，不含前导 `v`。 |
| `cache-hit` | 是否命中完全相同的安装缓存键，值为 `true` 或 `false`；跳过缓存步骤时为 `false`。 |
| `check-report` | runner 临时目录中 JSON 检查报告的绝对路径；跳过检查时为空。 |

布尔输入使用 `true` 或 `false`。Pinset 安装、按锁还原、本次要求的检查中任一步失败，工作流均失败。需要跳过检查时明确声明：

```yaml
- uses: Future-Element/pinset@v3.1.0
  with:
    version: '3.1.0'
    working-directory: packages/api
    skip-check: 'true'
```

## 按锁还原和缓存

Action 先解析、安装要求的 Pinset 发布版本，再还原工具安装缓存，随后始终执行 `pinset install`，缓存命中也不例外。`install` 只读取现有锁中的确切地址和摘要，不重新解析工具版本，并还原项目绑定、托管命令入口及锁要求的项目 Python 环境。

仅缓存 `PINSET_HOME/v3/installs`。缓存键包含 runner 操作系统、架构、镜像、确切 Pinset 版本、Pinset home 绝对路径和锁摘要。锁变化会使用新的缓存键；命中缓存仍校验收据并执行本次要求的检查。自定义 `PINSET_HOME` 时在 job 的 `env` 中设置，使后续步骤使用相同位置。

项目 `.pinset/local`、`.venv`、环境私钥、信任及加密 profile 状态不在缓存范围内。项目包依赖由构建步骤管理。自托管 runner 需要满足 `actions/cache@v5` 的版本要求，至少为 2.327.1。

默认执行 `pinset check --deep --probe --format json`，打印报告并通过 `check-report` 输出路径。`--deep` 重新计算安装内容指纹，与已验证的安装收据比较；`--probe` 明确运行有超时的版本探针并记录证据。这些检查不会执行应用构建。

即使工具检查失败，也可归档报告：

```yaml
- uses: Future-Element/pinset@v3.1.0
  id: pinset
- uses: actions/upload-artifact@v4
  if: ${{ always() && steps.pinset.outputs.check-report != '' }}
  with:
    name: pinset-check-${{ runner.os }}-${{ runner.arch }}
    path: ${{ steps.pinset.outputs.check-report }}
```

## JSON 检查契约

`pinset check --format json` 与 `pinset check --json` 使用同一报告结构，默认仍输出终端文本。`protocol` 为 `pinset/3`，`schema_version` 为 `1`。已有字段的名称、类型和含义保持稳定，后续允许增加字段；消费报告的脚本须容忍未知字段。

以下片段表示单个工具通过内容检查和版本探针，省略了观测证据等其他原有字段：

```json
{
  "protocol": "pinset/3",
  "schema_version": 1,
  "passed": true,
  "report": {
    "protocol": "pinset/3",
    "project": "/workspace/example",
    "checks": [
      {
        "tool": "node",
        "expected_version": "24.0.0",
        "actual_version": "24.0.0",
        "artifact_digest": {
          "expected": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          "receipt_matches_lock": true,
          "payload_verified": true,
          "passed": true
        },
        "passed": true,
        "configured": true,
        "installed": true,
        "bound": true,
        "actually_verified": true,
        "detail": "exact installation receipt matches"
      }
    ]
  }
}
```

| 字段 | 类型和含义 |
| --- | --- |
| `passed` | 布尔值；全部已选择工具均通过本次要求的检查。 |
| `report.checks[].tool` | 字符串；已选择工具名。 |
| `expected_version` | 字符串；锁中的确切版本。 |
| `actual_version` | 字符串或 `null`；明确指定 `--probe` 后解析到的实际版本。 |
| `artifact_digest.expected` | 字符串或 `null`；宿主平台产物的规范化锁定摘要，如 `sha256:<hex>` 或 SHA-512 SRI。 |
| `artifact_digest.receipt_matches_lock` | 布尔值；安装收据有效并与确切锁一致，包含产物摘要；收据缺失或无效时为 `false`。 |
| `artifact_digest.payload_verified` | 布尔值或 `null`；`--deep` 能检查安装内容时，表示其是否与收据一致；`null` 表示未检查。 |
| `artifact_digest.passed` | 布尔值；本次要求的收据和安装内容完整性检查通过。 |
| `source_verification` | 通用工具提供此字段；`status` 为 `signature-verified`、`digest-only` 或 `invalid`，`method`、`repository` 和 `passed` 描述锁中的来源证据。 |
| `report.checks[].passed` | 布尔值；工具已安装、已绑定，且本次要求的内容检查、版本探针通过。 |

`expected_version` 保留锁中版本，`actual_version` 是探针解析的版本。通用工具 tag 的 `v` 前缀、Python 构建标记和 Rust nightly 可能按各自规则规范化，因此不要只用两段字符串逐字相等判断结果；读取 `passed` 和 `actually_verified`。

未指定 `--probe` 时，`actual_version` 为 `null`、`actually_verified` 为 `false`。未指定 `--deep` 时，`payload_verified` 为 `null`。普通检查保持只读，不运行工具、联网或写入证据；`--probe` 明确允许在 Pinset home 中写入观测证据。

下载产物在安装时按锁校验。检查报告区分收据与锁的一致性、安装内容与收据的一致性，不声称重新计算了可能已不存在的原始下载包摘要。Java wrapper、Android 观测属于单独证据，不表示项目构建通过。

对于具备 GitHub Release attestation 的通用工具，普通 `check` 还会离线完成锁内 bundle 的密码学复验。`digest-only` 表示仅固定首次下载摘要，未验证发布方签名。来源证据无效会使工具检查失败，并阻止该工具的版本探针。支持范围和策略行为见 [自定义 CLI 指南](generic-tools.zh-CN.md)。

工具检查失败仍先打印完整报告，再以状态码 1 退出。锁缺失等操作错误也以状态码 1 退出，输出同样的协议、schema 版本和 `passed: false`，并包含带 `code`、`message` 的 `error` 对象。独立运行时可重定向 stdout 归档报告：

```sh
pinset check --deep --probe --format json > pinset-check.json
```


