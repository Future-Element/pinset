# Pinset 2.x 升级到 3.x

`scripts/migrate_v2_to_v3.py` 是独立升级脚本，需要 Python 3.11+，不接入 pinset 命令。升级期间停止其他 Pinset 操作。**不需要先手动安装 3.x**：脚本完成安装、环境迁移、验证、入口切换和旧文件清理。

```sh
# 预览整个升级计划，不写盘、不下载
python3 scripts/migrate_v2_to_v3.py

# 完成升级，默认包含全局选择及所有 2.x 登记项目
python3 scripts/migrate_v2_to_v3.py --apply

# 指定发布版本；默认推荐版本为 3.0.8
python3 scripts/migrate_v2_to_v3.py --apply --version 3.0.8

# 补充未登记项目；--project 可重复
python3 scripts/migrate_v2_to_v3.py --apply --project /path/to/project
```

`--registered` 仍接受，登记项目现在始终包含。不传 `--project` 时也会包含当前目录的 `pinset.toml`。Windows 使用 `py -3.11` 或更新的 Python。

升级依次执行：

1. 检查旧配置、精确锁、缓存和身份，备份源文件及确认属于 2.x 的命令入口。
2. 从官方 GitHub Release 下载 CLI 和配套 shim，核对 SHA-256、ZIP 内容及 CLI 版本，再安装到目标 `v3/bin`。目标已有 3.x 时复用，避免降级。版本检测使用两代均支持的 `--version`，不会把 3.x 参数发送给 2.x。
3. 转换配置、锁、加密 profile 和原生身份，复制已校验缓存，由 3.x 安装锁定 SDK。保留原选择器和精确版本，不重新解析 `latest`、`lts` 或 channel。
4. 对迁移环境执行 `check --deep`。已有 `.venv` 自动备份，记录 `pip freeze --all`，在新环境恢复依赖，执行 `pip check` 并核对包清单；失败就恢复原 `.venv`。
5. 全部成功后将识别到的旧 CLI、工具入口切换到 3.x。只替换确认属于 Pinset 的入口；其他程序不覆盖。旧 shim 对应命令在 3.x 不存在时移除；切换失败恢复原入口。
6. 清理已迁移项目的 `pinset.toml`、`pinset.lock`、旧加密 profile 和旧下载缓存。旧 `state/`、`installs/`、`cache/` 移入恢复备份，原位置不再留下这些 2.x 数据。备份保留精确 SDK 和历史状态，不自动删除。

pnpm 的 2.x 原生制品与 3.x 的安装布局不同。脚本保留锁定版本和原选择器，通过 3.x 官方解析器验证该精确版本的签名元数据，生成兼容制品锁，保留原锁定平台，并将转换前后记录写入备份的 `pnpm-lock-conversions.json`。此步骤需要访问 npm registry；使用 `--offline` 时会在修改环境前明确报错，需联网完成转换后重试。

备份位于 `<3.x 基础目录>/migration-backups/v2-to-v3-<时间>-<id>/`，默认 `~/.pinset/migration-backups/`。包含源文件及摘要、原命令文件、Python 包清单、SDK/历史状态和完成报告。旧 venv 保存在项目旁的 `.venv.pinset-v2-<id>`；恢复旧 venv 前需把归档 SDK 恢复到原路径。旧 SDK 收据不会改写成 3.x 收据。

| 选项 | 默认目录 |
| --- | --- |
| `--home`：2.x 数据目录 | 优先 `PINSET_HOME`；Windows 为 `%LOCALAPPDATA%/Pinset`；Unix/macOS 为 `$XDG_DATA_HOME/pinset` 或 `~/.local/share/pinset` |
| `--v3-home`：3.x 基础目录 | 优先 `PINSET_HOME`，否则 `~/.pinset`；实际数据使用其下的 `v3/` |

拒绝相对基础目录、符号链接状态路径，以及位于待清理旧目录内的目标/备份目录。

```sh
python3 scripts/migrate_v2_to_v3.py --home /old/pinset --v3-home /new/pinset --apply
```

Unix 自定义目标会在旧命令位置生成指定 `PINSET_HOME` 的转发脚本。Windows 自定义目标需在运行时将 `PINSET_HOME` 设为新基础目录，再用 `--home` 指定旧目录。`--legacy-pinset /path/to/pinset` 可明确指定需要替换的 2.x CLI；默认在 PATH 和旧默认安装目录识别。脚本不修改 shell profile 或 `.gitignore`。

离线或使用自行构建的 3.x 时可提供已有 CLI，旁边必须有配套 `pinset-shim`：

```sh
python3 scripts/migrate_v2_to_v3.py --apply --offline --pinset /path/to/v3/pinset
```

`--offline` 禁止 CLI/SDK 下载；目标已安装 3.x 时可省略 `--pinset`。所需 SDK 制品必须有已校验缓存，Python 依赖需有可用的本地 wheel/index，例如 `PIP_FIND_LINKS`。联网模式会下载缺失的锁定制品和 Python 包。

加密 profile 复制到 `.pinset/env/<name>.env`，只调整版本封装，age 密文保持一致，不解密变量。私钥通过原生凭据库在内存中复制到 3.x 服务，不进入文件、命令参数或报告；旧凭据保留用于恢复。macOS 使用 Keychain，Windows 使用 Credential Manager，Linux 需要已解锁的 Secret Service 和 `secret-tool`。Python 包清单可能包含私有索引 URL，仅写入权限受限的备份，不打印 pip 输出。

项目 ID、共享/本地 profile 选择、Rust 选项及支持的策略会保留；workspace 和显式全局继承展开为独立项目配置。旧 trust 不自动转移，检查新配置后在项目执行 `pinset env trust add`。旧 access 历史、tasks、验证声明、变量契约、自定义 Python 配置列入提示并留在备份；自定义/命名 Python 环境目录不删除。

部分迁移必须使用 `--keep-v2`，保留旧文件和入口，跳过切换、清理：

```sh
python3 scripts/migrate_v2_to_v3.py --apply --keep-v2 --no-install
python3 scripts/migrate_v2_to_v3.py --apply --keep-v2 --no-global --project /path/to/project
```

`--no-install`、`--no-global`、`--skip-unsupported`、`--skip-identities` 都要求 `--keep-v2`。跳过身份后必须提供匹配的 `PINSET_IDENTITY`。3.x 移除的工具默认拒绝升级；显式跳过后保留在 2.x。明文 profile、受管变量覆盖、锁不一致、身份冲突都会拒绝迁移。

已有 3.x 配置不覆盖。完整升级只允许继续本脚本留下的、源文件摘要一致且已完成安装的迁移收据，再重新验证。无收据或只迁移未安装的配置需 `--keep-v2`。普通失败可以重跑：已完成项目保留结果，失败项目撤回本次新配置并恢复旧 `.venv`；全部环境验证成功后才切换和清理。

断电或强制结束可能留下 `.pinset/local/migration-v2-incomplete.json` 或 `<3.x 基础目录>/upgrade-v2-pending.json`，脚本会拒绝继续清理。停止 Pinset 操作，根据标记、备份 `manifest.json`、`binaries.json`、`upgrade-result.json` 恢复：只移除本次生成且摘要一致的配置，保留编辑；移开失败的新 venv，恢复旧环境。切换/清理中断时，从 `legacy-home/` 恢复旧目录，从 `binaries.json` 恢复文件、权限、链接，确认一致后移除未完成标记，再重跑。

升级后可直接使用原 PATH 中的命令；新 shell 也可用 3.x `self shell` 将 `v3/bin` 放到 PATH 最前面。提交 `.pinset/config.toml`、`.pinset/lock.toml`、`.pinset/env/`，忽略 `.pinset/local/`、`.venv/` 和恢复备份目录。测试接入 Docker `fast`，包含完整升级、失败恢复、真实 3.x 离线安装/路由验证；原生凭据仍需各平台实际验收。
