# 在 WSL 中构建和测试 Pinset

WSL 按独立 Linux 环境处理。Windows 编译出的 PE 可执行文件不能作为 Linux 原生 Pinset 使用；请直接下载 GitHub Release 的 Linux x64 归档，或在 WSL 中构建 ELF。

## 直接测试 GitHub Release

下载并解压 `pinset-linux-x86_64.tar.gz`：

```bash
tar -xzf pinset-linux-x86_64.tar.gz
chmod +x pinset pinset-shim
./pinset --version
file pinset pinset-shim
```

先做不安装真实 Node 的安全检查：

```bash
TEST_HOME="$(mktemp -d)"
export PINSET_HOME="$TEST_HOME"

./pinset source list node
./pinset init
./pinset doctor
```

`doctor` 在尚未选择版本时报告项目未配置 Node 是预期行为。测试后可退出当前 shell；临时目录不会影响 Windows 或默认 WSL 数据目录。

真实 Node 安装测试请在你准备好的 WSL 测试目录内自行执行：

```bash
mkdir -p "$HOME/pinset-mvp-test"
cd "$HOME/pinset-mvp-test"
/path/to/pinset init
/path/to/pinset use node@24.0.0
/path/to/pinset exec -- node --version
```

## 在 WSL 中从源码构建

Ubuntu 安装构建依赖：

```bash
sudo apt update
sudo apt install -y build-essential curl ca-certificates git pkg-config liblzma-dev
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"
rustup update stable
```

Pinset 的最低 Rust 版本是 1.85。建议把源码放到 WSL 的 ext4 文件系统，而不是直接在 `/mnt/c` 编译：

```bash
git clone git@github.com:Future-Element/pinset.git "$HOME/src/Pinset"
cd "$HOME/src/Pinset"
cargo build --release --locked -p pinset-cli -p pinset-shim
```

产物：

```text
target/release/pinset
target/release/pinset-shim
```

验证它们是 Linux ELF：

```bash
file target/release/pinset target/release/pinset-shim
ldd target/release/pinset
target/release/pinset --version
```

运行项目测试不会安装真实运行时：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

这些测试只使用临时目录、本地假 HTTP 服务、构造归档和假运行时。完整命令见 [MVP 使用指南](USAGE.md)。
