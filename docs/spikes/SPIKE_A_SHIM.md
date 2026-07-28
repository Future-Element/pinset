# Spike A：跨平台 shim

日期：2026-07-28  
状态：**Provisional — Windows 功能通过，性能门槛未通过，其他平台未验证**

## 1. 验证问题

1. 一个极小 Rust shim 能否根据当前目录找到最近的 `pinset.toml`？
2. 同一 shim 二进制能否通过调用文件名表现为 `node`、`npm` 等命令？
3. 是否可以在用户目录安装 shim，而不需要 Windows 管理员权限？
4. 参数、环境和子进程退出码能否正确传递？
5. 能否阻止 shim 递归？
6. 额外路由开销能否达到 p95 ≤ 10 ms 的候选目标？

## 2. 实现

最小 Cargo workspace：

- `pinset-core`
  - 向祖先目录查找最近 `pinset.toml`；
  - 只接受 `schema = 1` 和 `[tools]`，未知/可执行式字段直接拒绝；
  - 从 `PINSET_HOME/installs/<tool>/<version>/<target>/bin/` 解析命令；
  - 在用户目录以硬链接安装多调用 shim，失败时回退为安全复制；
  - 拒绝路径式命令名、重复命令及覆盖既有文件。
- `pinset-shim`
  - 从 `argv[0]` 推导命令名；
  - 设置选中工具、版本和配置来源环境变量；
  - 透传参数和真实退出码；
  - 使用 `PINSET_SHIM_DEPTH` 阻止递归。
- `pinset`
  - 最小 `which`、`current`；
  - `shim install` 用于创建多调用入口。

本 spike 只支持 Node 命令族和假运行时，不含下载器、锁文件或真实 Node provider。

## 3. 自动化结果

Windows x64，本机 Rust 1.96.0：

```text
cargo test --workspace
14 passed, 0 failed

cargo clippy --workspace --all-targets -- -D warnings
passed

cargo fmt --all -- --check
passed
```

覆盖：

- 最近祖先配置优先；
- 未知配置字段和未知 schema 拒绝；
- 缺失运行时给出完整搜索路径；
- shim 硬链接/复制安装；
- 不覆盖既有同名命令；
- 拒绝 `../node` 和重复命令；
- debug 调用与真实 `node.exe` 多调用入口；
- 假 Node 版本选择；
- 递归调用拒绝；
- 子进程退出码 42 保持不变。

## 4. 性能结果

### 4.1 核心解析

命令：

```shell
cargo run --release -p pinset-core --example resolve_bench -- 5000
```

结果：

```text
iterations=5000
median_us=99
p95_us=195
p99_us=289
```

结论：祖先查找、TOML 解析和路径解析不是当前瓶颈。

### 4.2 完整进程链

方法：

- 将 release `pinset` 二进制复制为假 `node.exe`；
- 比较直接启动该假运行时与通过实际多调用 `node.exe` shim 启动；
- 预热 20 次，随后交错采样 1000 次；
- stdout/stderr 定向到 null，减少输出管道噪声。

命令：

```shell
cargo build --release -p pinset-cli -p pinset-shim
cargo run --release -p pinset-shim --example process_bench -- 1000
```

结果：

```text
iterations=1000
direct_median_us=6164
direct_p95_us=7588
shimmed_median_us=14188
shimmed_p95_us=25606
estimated_p95_overhead_us=18017
```

结论：

- median 差值约 8.0 ms；
- p95 差值估算约 18.0 ms；
- 当前 Windows x64 环境没有达到 p95 ≤ 10 ms 的候选目标；
- p95 受 Windows 调度、安全扫描和额外进程创建影响明显，但这是用户真实会承担的路径，不能只用核心函数基准替代。

这些数字只代表当前机器和未签名 spike 二进制，不可外推到 macOS、Linux 或正式签名发布物。

## 5. 安全结论

已确认：

- 日常选择和执行无需写系统目录或创建系统级 symlink；
- shim 安装不会覆盖目标目录中已有的 `node` 等文件；
- 命令名不能逃逸 shim 目标目录；
- 解析热路径不需要网络；
- 深度保护能终止 shim → shim 递归；
- 项目配置不能携带脚本或未知顶层行为。

尚未覆盖：

- 符号链接/junction 指向 shim 目录的文件身份检测；
- 配置查找跨挂载点、权限失败和 symlink cwd 的完整规则；
- Windows 长路径、保留名和大小写碰撞；
- 并发重装 shim 的事务语义；
- 发布签名二进制对安全扫描延迟的影响。

## 6. 决策

暂时保留 shim 作为主方案，但 D-010 继续保持 **Provisional**，不能因功能测试通过就视为架构已接受。

下一轮对比：

1. macOS/Linux 上运行同一功能和基准；
2. Windows release 配置使用 LTO/strip 后重测；
3. 测试 Windows 原生进程启动实现是否有实质改善；
4. 对比 Shell activation 在目录切换时更新 PATH 的成本、Shell 覆盖和父进程限制；
5. 对比“全局直接 PATH + 项目命令显式 `pinset exec`”作为 Windows 降级策略；
6. 使用真实 Node 启动时间评估相对影响，但仍保留绝对开销指标。

## 7. Go/No-Go

- 功能正确性：Windows x64 **Go**
- 无管理员权限：当前用户目录测试 **Go**
- 核心解析性能：**Go**
- 完整 shim 性能：**No-Go，待方案比较**
- macOS/Linux：**Not verified**

因此 Spike A 不能关闭，但不阻塞可独立开展的 Spike B 安装事务验证。
