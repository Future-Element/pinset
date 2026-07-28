# Spike B：事务安装内核

日期：2026-07-28  
状态：**Provisional — 本地 HTTP/ZIP 与源切换事务通过，真实产物与跨平台未验证**

## 1. 本阶段验证问题

1. 下载内容是否在进入安装目录前强制通过 SHA-256？
2. 坏哈希、截断响应或恶意归档是否可能留下可选择的半安装？
3. ZIP 是否能阻止路径穿越、特殊文件和重复/大小写冲突条目？
4. 解压限制是否依据实际输出流，而不是盲信归档头部？
5. 最终安装是否只通过一次原子 rename 对外可见？
6. 下载器依赖能否与高频执行的 shim 隔离？
7. 国内镜像或企业代理能否只改变传输地址，不改变可信产物身份？

本阶段没有连接 Node.js、Python 或 Flutter 上游，也没有冻结公开 CLI。

## 2. 实现

核心代码位于 `pinset-core::installer`，默认不编译，通过 `installer` Cargo feature 启用。

安装流程：

```text
校验请求路径
  → 创建 PINSET_HOME/tmp/install-* 唯一事务目录
  → 按本机选择构造有序 source 候选
  → HTTP 下载并流式计算 SHA-256
  → 哈希完全一致后打开 ZIP
  → 安全解压到事务 payload
  → 检查 provider 声明的必需文件
  → 写入脱敏安装收据
  → 原子 rename 到 installs/<tool>/<version>/<target>
  → 清理下载和事务目录
```

采用的依赖：

- [reqwest 0.13.4](https://crates.io/crates/reqwest/0.13.4)：阻塞客户端、rustls、系统代理；
- [sha2 0.11](https://crates.io/crates/sha2/0.11.0)：RustCrypto SHA-256；
- [zip 6.0](https://crates.io/crates/zip/6.0.0)：只启用 deflate，避免默认引入其他压缩和加密实现；
- `tempfile`：同一 Pinset 数据根中的唯一事务目录。
- [atomic-write-file 0.3](https://crates.io/crates/atomic-write-file/0.3.0)：同目录临时文件与原子 replace，用于 `sources.toml`；
- [url 2.5](https://crates.io/crates/url/2.5.8)：解析、验证和规范化自定义源 base URL。

zip 6.0 选择与当前 Rust 1.85 MSRV 兼容；后续升级需要重新运行恶意归档 fixture。

## 3. 安全边界

请求阶段：

- `tool`、`version`、`target` 不能包含路径分隔符、盘符或 `..`；
- 必须声明至少一个安装后必需文件；
- SHA-256 必须是精确 32 字节十六进制；
- 最终目录已存在时拒绝覆盖。

下载阶段：

- 先检查 Content-Length，再对实际读取字节持续执行大小限制；
- 写入唯一且 `create_new` 的临时文件；
- 下载文件在完成后 `sync_all`；
- 哈希不一致不可降级为警告；
- 错误信息和收据移除 URL 用户信息、查询参数与 fragment。
- canonical URL 与实际下载源分开记录；
- 只有网络请求/响应失败才尝试下一个源；
- SHA-256 不匹配立即停止，不进行源回退。

解压阶段：

- `enclosed_name` 和相对路径组件双重检查；
- 拒绝 symlink 和其他特殊 Unix 文件类型；
- 拒绝重复路径；Windows 上按不区分大小写检测碰撞；
- 文件使用 `create_new`，不覆盖先前条目；
- 同时限制 entry 数量、声明展开大小和实际输出字节；
- 每个文件写完后执行 `sync_all`。

提交阶段：

- 最终安装目录在完整校验前不存在；
- 必需运行时文件缺失时不提交；
- `.pinset-install.toml` 与 payload 一起原子出现；
- 同名最终目录在 rename 前再次检查，竞争失败不会覆盖已有安装；
- `TempDir` 负责失败和成功后的事务残留清理。

## 4. 自动化结果

Windows x64，本机 Rust 1.96.0：

```text
cargo test -p pinset-core --features node-provider,sources
24 passed, 0 failed

cargo test -p pinset-cli
3 passed, 0 failed

cargo test -p pinset-shim
5 passed, 0 failed

cargo clippy -p pinset-core --all-targets --features node-provider,sources -- -D warnings
cargo clippy -p pinset-cli -p pinset-shim --all-targets -- -D warnings
passed

cargo fmt --all -- --check
passed
```

本轮按用户边界没有重新运行 `installer` feature 的安装事务测试；以上 32 项只使用配置、字符串、临时目录和 fake runtime，不下载或安装真实运行时。安装器代码未被本轮修改。

安装器覆盖：

- 本地 HTTP 正常下载、校验、解压和原子提交；
- 收据 URL token 脱敏；
- 显式镜像源成功安装；
- 首选源连接失败后回退到第二个源；
- 首选源哈希失败时禁止回退；
- SHA-256 错误；
- HTTP 声明长度大于实际内容的中断响应；
- `../escape.txt` ZIP 路径穿越；
- ZIP 实际展开大小超限；
- 安装版本路径穿越；
- 所有失败场景最终安装目录不存在；
- 所有失败场景临时事务目录为空。

依赖隔离检查：

```text
cargo check -p pinset-shim
cargo tree -p pinset-shim -e normal
```

结果：shim 的正常依赖树不包含 reqwest、zip、sha2、url 或 atomic-write-file。

安装源配置覆盖：

- 缺少 `sources.toml` 时只返回内置 official 源，不创建文件；
- 添加、切换、有序 fallback、清空 fallback、删除和保存后重载；
- 内置 official 源不可覆盖或删除；
- HTTPS 默认要求，以及 HTTP 显式 `--allow-insecure`；
- 拒绝 URL 凭据、query、fragment、未知字段和未知 schema；
- 拒绝悬空、重复或包含活动源的 fallback；
- 活动源或 fallback 中的源不可删除；
- CLI 集成测试只使用随机临时 `PINSET_HOME`，不创建真实安装目录，不访问远端网络。

Node provider 计划覆盖：

- Windows x64 ZIP 产物路径、canonical URL 和归档内 `node.exe` 路径；
- Linux x64 与 Linux arm64 的 `tar.xz` 产物路径；
- macOS x64/arm64 目标映射由同一静态映射覆盖；
- 自定义 active 源与 official fallback 保持相同 artifact path；
- canonical URL 不受活动镜像影响；
- 浮动版本、预发布版本和未知 target 在任何网络访问前被拒绝；
- 危险 artifact path 不能逃逸或重写 source base URL。

## 5. 尚未验证

- Node.js 官方 ZIP 的真实目录布局、签名 SHASUMS 和版本执行验证；
- Flutter ZIP 与更大的归档/文件数量；
- Python 和 Unix 所需 tar.gz、tar.xz、tar.zst；
- macOS/Linux 权限位和可执行文件恢复；
- Windows 长路径、保留名、ADS 和更多大小写/Unicode 碰撞；
- 多进程同时安装同一版本的显式文件锁；
- 下载缓存、断点续传和离线模式；
- 内置社区 preset 的签名发布、更新和下线机制；
- `source test` 的只读网络诊断与结构化结果；
- Node `dist/index.json` 的可信版本解析、签名 SHASUMS 和 lock 写入；
- 已存在且收据完全一致时的幂等成功语义；
- 目录 fsync 与突然断电后的持久性保证；
- 上游签名、attestation 和许可证元数据。

## 6. 结论

- 失败不暴露半安装：**Go（本地 Windows fixture）**
- SHA-256 强制校验：**Go**
- ZIP 基础安全边界：**Go**
- shim 依赖隔离：**Go**
- 安装源与可信哈希分离：**Go**
- 网络错误回退/校验错误硬停止：**Go**
- 三种真实产物：**Not verified**
- 完整跨平台安装器：**Not verified**

因此事务内核可以作为 Spike B 的基础继续使用，但整个 Spike B 仍为 Provisional。

## 7. 下一步

1. 在 WSL/CI 中验证 Linux 二进制编译、source CLI 和 Node artifact plan；
2. 对 `sources.toml` schema 与当前 `pinset source` CLI 做跨平台评审；
3. 在隔离 CI 中接入一个固定 Node.js Windows x64 官方 ZIP；
4. 在隔离 CI 中用官方源与一个显式社区镜像验证相同 lock/哈希；
5. 下载并验证官方签名 SHASUMS，再校验归档 SHA-256；
6. 验证归档布局、`node --version` 和附带 `npm`/`npx`；
7. 添加跨进程锁和完全一致安装的幂等语义；
8. 再扩展 tar 系列归档，为 macOS/Linux、Python、Flutter 做准备。
