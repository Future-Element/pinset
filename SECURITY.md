# Security Policy

## Supported versions

Pinset 只对最新发布的 GitHub Release 提供安全修复。仓库中的版本号不代表对应 Release 已经发布；报告问题时请同时注明实际安装版本和提交。

## Reporting a vulnerability

请不要在公开 Issue 中披露尚未修复的漏洞。优先使用 GitHub 仓库的 Private vulnerability reporting；如果该入口暂不可见，请联系仓库维护者并仅说明需要建立私密沟通渠道，不要在首次公开消息中附带漏洞细节、利用代码、令牌或用户数据。

报告建议包含：

- 受影响版本和平台；
- 最小复现条件；
- 可能影响的文件、命令或信任边界；
- 是否存在已知利用；
- 建议的缓解方式（如有）。

维护者确认安全沟通渠道后，再交换完整细节。修复发布前会尽量协调披露时间。

## Release security gate

- Pull Request 与 Release 工作流必须对提交的 `Cargo.lock` 执行固定版本的 `cargo audit --deny warnings`。
- 最新 RustSec 数据库中的漏洞、停止维护和撤回告警会阻止合并与发布。只有无修复版本、低于高危且经代码路径证明不可达的告警才能建立逐项记录的临时例外；不得通过忽略规则绕过高危运行时依赖告警。
- GitHub Dependabot 标记为高危的运行时依赖必须在创建正式版本标签前升级或移除。Pinset 不发布带有未解决高危运行时依赖告警的版本。

当前唯一例外是 `RUSTSEC-2023-0071`：它影响 RSA 私钥操作的可观察计时，而 Pinset 仅使用 `pgp` 解析公开 Node.js 发布证书并验证公开签名，不持有 OpenPGP 私钥或执行解密。上游提供修复后必须移除此例外。
