# 在 WSL 中构建和安全测试 Pinset

本文只构建 Pinset 自身并测试配置、路由和 Node 产物计划，不下载或安装真实 Node、Python、Flutter，也不修改 Windows PATH、Shell 或现有版本管理器。

当前已确认的本机 WSL 环境是 Ubuntu、WSL2、x86_64。WSL 中尚未安装 Rust 和 GCC，以下安装步骤需要由用户自行执行。

## 1. 准备 WSL 构建环境

在 Ubuntu WSL 中执行：

```bash
sudo apt update
sudo apt install -y build-essential curl ca-certificates git rsync

curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"

rustc --version
cargo --version
gcc --version
```

项目声明的最低 Rust 版本是 1.85。使用当前 stable Rust 即可；若 `rustc --version` 低于 1.85，执行：

```bash
rustup update stable
rustup default stable
```

## 2. 把源码同步到 WSL 文件系统

可以直接在 `/mnt/c` 构建，但 Cargo 会产生大量小文件，速度和 Linux 权限语义都不理想。建议从 Windows 工作区同步到 WSL 的 ext4 文件系统：

```bash
mkdir -p "$HOME/src/Pinset"
rsync -a \
  --exclude '.git/' \
  --exclude 'target/' \
  /mnt/c/Users/zhoub/Code/Future-Element/Pinset/ \
  "$HOME/src/Pinset/"

cd "$HOME/src/Pinset"
```

这条命令不会删除 WSL 目标目录中的额外文件。后续代码变化时可以再次执行相同的 `rsync`。

## 3. 构建 Linux x86_64 二进制

在 WSL 源码目录执行：

```bash
cargo build --release --locked -p pinset-cli -p pinset-shim
```

生成文件：

```text
target/release/pinset
target/release/pinset-shim
```

确认它们是 Linux ELF，而不是 Windows PE：

```bash
file target/release/pinset target/release/pinset-shim
ldd target/release/pinset
./target/release/pinset --version
```

WSL x86_64 原生构建对应 Rust target `x86_64-unknown-linux-gnu`。不建议在 Windows MSVC 工具链中直接加 `--target x86_64-unknown-linux-gnu`：仅安装 Rust target 不会自动提供 Linux glibc linker。

## 4. 不安装运行时的安全测试

下面的测试不启用 `installer` feature：

```bash
cargo test -p pinset-core --features node-provider,sources
cargo test -p pinset-cli
cargo test -p pinset-shim
cargo clippy -p pinset-core --all-targets \
  --features node-provider,sources -- -D warnings
cargo clippy -p pinset-cli -p pinset-shim \
  --all-targets -- -D warnings
```

手动测试 source CLI 时，为 Pinset 指定一次性目录：

```bash
PINSET_TEST_HOME="$(mktemp -d)"
export PINSET_HOME="$PINSET_TEST_HOME"

./target/release/pinset source list node
./target/release/pinset source add node example \
  --base-url https://mirror.example/node/
./target/release/pinset source use node example
./target/release/pinset source fallback node official
./target/release/pinset source list node

test ! -e "$PINSET_TEST_HOME/installs"
test ! -e "$PINSET_TEST_HOME/shims"
printf 'temporary PINSET_HOME=%s\n' "$PINSET_TEST_HOME"
```

`mirror.example` 是保留的示例域名；上述命令只保存 URL，不会连接它。确认结果后可以手动删除打印出的临时目录。

## 5. 当前 Linux 能测试和不能测试的边界

可以测试：

- Linux 原生编译和启动；
- `source list/add/use/fallback/remove`；
- Node Windows/Linux/macOS 产物路径与源候选的单元测试；
- fake runtime 的 shim 路由、递归保护和退出码传递。

暂时不能测试：

- 真实 Node 下载与安装 CLI；
- Linux `tar.xz` 解压和权限恢复；
- Node 签名 `SHASUMS256.txt` 验证；
- Shell profile/PATH 自动接管；
- Python 和 Flutter 真实安装。
