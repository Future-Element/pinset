# Local verification / 本地验证

Build the pinned Linux environment once and retain its Cargo and Node dependency caches. This container does not publish artifacts, modify the original checkout, or replace Windows/macOS native acceptance.

先构建固定基础镜像的 Linux 验证环境，并保留 Cargo 与 Node 缓存。下面在当前独立开发工作目录中操作，格式化会写回该目录；构建产物和依赖放入专用 Docker 卷。

```powershell
docker build -t pinset-environment-verify:2.13 -f scripts/docker/Dockerfile.verify scripts/docker
docker run --detach --init --name pinset-environment-dev --cpus 8 --memory 10g --mount "type=bind,source=$((Get-Location).Path),target=/workspace" --mount type=volume,source=pinset-cargo-registry,target=/usr/local/cargo/registry --mount type=volume,source=pinset-environment-target,target=/workspace/target --mount type=volume,source=pinset-environment-extension-deps,target=/workspace/editors/vscode/node_modules --mount type=volume,source=pinset-environment-website-deps,target=/workspace/website/node_modules pinset-environment-verify:2.13
docker exec pinset-environment-dev cargo fmt --all
docker exec pinset-environment-dev sh scripts/docker/verify.sh rust
docker exec pinset-environment-dev sh scripts/docker/verify.sh extension
docker exec pinset-environment-dev sh scripts/docker/verify.sh website
```

Native Linux execution acceptance / Linux 实际执行验收：

```powershell
docker exec pinset-environment-dev sh scripts/docker/verify.sh native
```

Native acceptance reuses content-addressed SDK archives and VS Code downloads in `/var/cache/pinset-acceptance` inside the retained container. Each run creates fresh projects, installations, venvs, trust records and editor state. Docker Desktop's WSL kernel prompt is suppressed only in this container test entry point.

Re-run focused checks after fixes; run the complete relevant suite before merging each development version. Keep required CI and platform-native checks consolidated after Docker succeeds. Do not infer a tested SSH/WSL session from a Linux container result.

修复后先运行相关检查，每个开发版本合并前完成对应完整检查，再集中运行分支保护及跨平台验收。Linux 容器验证不代表 SSH、WSL、Windows 或 macOS 原生会话已经通过。
