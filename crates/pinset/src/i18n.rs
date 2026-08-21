use std::{error::Error as StdError, fmt, path::Path, str::FromStr};

use pinset_core::{Error, LockAuditSummary};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Language {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" => Ok(Self::English),
            "zh" | "zh-cn" | "zh_hans" | "zh-hans" => Ok(Self::SimplifiedChinese),
            _ => Err(format!(
                "unsupported language {value:?}; expected en or zh-CN / 不支持的语言 {value:?}，可选 en 或 zh-CN"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Catalog {
    language: Language,
}

impl Catalog {
    pub const fn new(language: Language) -> Self {
        Self { language }
    }

    pub const fn language(self) -> Language {
        self.language
    }

    pub fn error(self, detail: impl fmt::Display) -> String {
        match self.language {
            Language::English => format!("error: {detail}"),
            Language::SimplifiedChinese => format!("错误：{detail}"),
        }
    }

    pub fn command_error(self, error: &(dyn StdError + 'static)) -> String {
        if self.language == Language::English {
            return self.error(error);
        }
        let Some(error) = error.downcast_ref::<Error>() else {
            return self.error(error);
        };
        match error {
            Error::ProjectConfigNotFound { start } => {
                format!("错误：从 {} 向上未找到 pinset.toml", start.display())
            }
            Error::ReadProjectConfig { path, source } => {
                format!("错误：无法读取项目配置 {}：{source}", path.display())
            }
            Error::ParseProjectConfig { path, source } => {
                format!("错误：项目配置 {} 格式无效：{source}", path.display())
            }
            Error::UnsupportedSchema { actual } => {
                format!("错误：不支持 pinset.toml schema {actual}，当前支持 schema 1、2 和 3")
            }
            Error::GlobalConfigNotFound { path } => {
                format!("错误：全局配置不存在：{}", path.display())
            }
            Error::ReadGlobalConfig { path, source } => {
                format!("错误：无法读取全局配置 {}：{source}", path.display())
            }
            Error::ParseGlobalConfig { path, source } => {
                format!("错误：全局配置 {} 格式无效：{source}", path.display())
            }
            Error::UnsupportedGlobalConfigSchema { actual } => {
                format!("错误：不支持 global.toml schema {actual}，当前支持 schema 1、2 和 3")
            }
            Error::ReadUserSettings { path, source } => {
                format!("错误：无法读取用户设置 {}：{source}", path.display())
            }
            Error::ParseUserSettings { path, source } => {
                format!("错误：用户设置 {} 格式无效：{source}", path.display())
            }
            Error::UnsupportedUserSettingsSchema { actual } => {
                format!("错误：不支持 settings.toml schema {actual}，当前仅支持 schema 1")
            }
            Error::UnsupportedCommand { command } => {
                format!("错误：当前不支持命令 {command:?}")
            }
            Error::ToolNotConfigured { tool, config_path } => {
                format!("错误：工具 {tool:?} 未在 {} 中声明", config_path.display())
            }
            Error::ProjectToolSelectionRequired { tool, config_path } => format!(
                "错误：严格项目 {} 未声明工具 {tool:?}；请运行 `pinset use {tool}@<选择器>`，或在项目策略中显式启用回退",
                config_path.display()
            ),
            Error::ToolSelectionNotFound {
                tool,
                start,
                global_config_path,
            } => format!(
                "错误：未找到工具 {tool:?} 的版本选择；已检查 {} 的项目祖先目录和全局配置 {}",
                start.display(),
                global_config_path.display()
            ),
            Error::CommandSelectionNotFound { command, .. } => {
                format!("错误：项目、全局设置和系统 PATH 中都未找到命令 {command:?}")
            }
            Error::RuntimeCommandNotFound {
                tool,
                version,
                command,
                searched,
            } => format!(
                "错误：已选择 {tool}@{version}，但尚未安装命令 {command:?}；已检查：{searched}"
            ),
            Error::PinsetHomeUnavailable => {
                "错误：无法确定 Pinset 数据目录，请显式设置 PINSET_HOME".to_owned()
            }
            Error::InvalidNodeVersion { version } => {
                format!("错误：Node.js 版本 {version:?} 无效，必须使用不带 v 前缀的 x.y.z")
            }
            Error::InvalidNodeSelector { selector } => format!(
                "错误：Node.js 选择器 {selector:?} 无效，可使用 x.y.z、主版本、主次版本、lts 或 current"
            ),
            Error::NodeSelectorNotFound { selector } => {
                format!("错误：Node.js 官方索引中没有与 {selector:?} 匹配且支持全部目标平台的版本")
            }
            Error::InvalidNodeIndex { reason } => {
                format!("错误：Node.js 官方版本索引无效：{reason}")
            }
            Error::InvalidPythonVersion { version } => {
                format!("错误：Python 精确发行版 {version:?} 无效，应为 x.y.z+YYYYMMDD")
            }
            Error::InvalidPythonSelector { selector } => format!(
                "错误：Python 选择器 {selector:?} 无效，可使用 x.y.z、主版本、主次版本、latest 或 current"
            ),
            Error::PythonSelectorNotFound { selector } => {
                format!(
                    "错误：官方注册表中没有与 {selector:?} 匹配且支持全部目标平台的稳定 Python 发行版"
                )
            }
            Error::InvalidPythonIndex { reason } => {
                format!("错误：Python 官方版本注册表无效：{reason}")
            }
            Error::PythonEnvironmentNotOwned { path } => {
                format!(
                    "错误：项目虚拟环境 {} 不属于 Pinset，拒绝接管",
                    path.display()
                )
            }
            Error::PythonEnvironmentMismatch {
                path,
                expected,
                actual,
            } => format!(
                "错误：项目虚拟环境 {} 使用 {actual}，但项目锁定 {expected}；请显式运行 `pinset venv recreate`",
                path.display()
            ),
            Error::PythonEnvironmentMissing { path } => format!(
                "错误：项目虚拟环境 {} 不存在；请运行 `pinset venv create`",
                path.display()
            ),
            Error::PythonEnvironmentSelectionMissing { path } => format!(
                "错误：{} 中没有项目级 Python 选择，无法管理虚拟环境",
                path.display()
            ),
            Error::NodeVersionNotInstalled { version } => {
                format!("错误：Pinset 未安装 Node.js {version}")
            }
            Error::NodeVersionInUse {
                version,
                references,
            } => format!(
                "错误：Node.js {version} 仍被以下配置引用，拒绝卸载：{references}；确认接受配置失效后可使用 --force"
            ),
            Error::UnsafeNodeInstallEntry { path } => format!(
                "错误：安装目录 {} 缺少匹配收据或不是 Pinset 可安全删除的目录，已停止卸载",
                path.display()
            ),
            Error::ToolVersionNotInstalled { tool, version } => {
                format!("错误：Pinset 未安装 {tool}@{version}")
            }
            Error::ToolVersionInUse {
                tool,
                version,
                references,
            } => format!(
                "错误：{tool}@{version} 仍被以下配置引用，拒绝卸载：{references}；确认接受配置失效后可使用 --force"
            ),
            Error::UnsafeToolInstallEntry { tool, path } => format!(
                "错误：{tool} 安装目录 {} 缺少匹配收据或不是 Pinset 可安全删除的目录，已停止操作",
                path.display()
            ),
            Error::UnsafeDownloadCacheEntry { path } => {
                format!(
                    "错误：下载缓存项 {} 不是普通文件，已拒绝操作",
                    path.display()
                )
            }
            Error::LockfileMismatch {
                selection_path,
                tool,
                configured,
                locked,
            } => format!(
                "错误：{} 选择了 {tool}@{configured}，但锁文件中是 {tool}@{locked}，请重新生成匹配的锁文件",
                selection_path.display()
            ),
            Error::SourceNotFound { provider, alias } => {
                format!("错误：{provider} 的安装源 {alias:?} 不存在")
            }
            Error::InvalidSourceBaseUrl { url, reason } => {
                format!("错误：安装源地址 {url:?} 无效：{reason}")
            }
            Error::VerificationPolicyViolation {
                tool,
                required,
                actual,
            } => format!("错误：{tool} 的验证强度为 {actual}，低于项目要求的 {required}"),
            Error::VerificationDowngrade {
                tool,
                previous,
                next,
            } => format!(
                "错误：拒绝把 {tool} 的验证强度从 {previous} 降为 {next}；请保留更强证据或显式创建全新锁"
            ),
            Error::ReleaseAgeUnavailable { tool } => format!(
                "错误：{tool} 的 Provider 没有提供发布时间，无法执行 minimum-release-age 策略"
            ),
            Error::ReleaseTooNew {
                tool,
                released_at,
                required,
            } => format!("错误：{tool} 发布于 {released_at}，尚未满足最短发布年龄 {required}"),
            Error::ProviderDependencyMissing { tool, dependency } => {
                format!("错误：{tool} 依赖 {dependency}，请先为当前作用域选择并锁定 {dependency}")
            }
            Error::ProviderDependencyUnknown { tool, dependency } => {
                format!("错误：{tool} 声明了未知 Provider 依赖 {dependency}")
            }
            Error::ProviderDependencyCycle { cycle } => {
                format!("错误：Provider 依赖图存在循环：{cycle}")
            }
            Error::ReadProviderRegistry { path, source } => format!(
                "错误：无法读取 Provider Registry {}：{source}",
                path.display()
            ),
            Error::ProviderRegistrySignatureInvalid { reason } => {
                format!("错误：Provider Registry 签名无效：{reason}")
            }
            Error::ProviderRegistryInvalid { reason } => {
                format!("错误：Provider Registry 内容无效：{reason}")
            }
            _ => format!("错误：操作失败：{error}"),
        }
    }

    pub fn language_saved(self, path: &Path) -> String {
        match self.language {
            Language::English => format!(
                "language set to {} in {}",
                self.language.as_str(),
                path.display()
            ),
            Language::SimplifiedChinese => {
                format!("语言已切换为中文，设置保存在 {}", path.display())
            }
        }
    }

    pub fn top_level_help(self) -> &'static str {
        match self.language {
            Language::English => {
                "Pinset manages predictable runtime versions.\n\nUsage: pinset [--lang <en|zh-CN>] <COMMAND>\n\nCommands:\n  init         Create project configuration\n  detect       Detect traditional version files\n  import       Import traditional version selections\n  global       Show or batch-set global defaults\n  use          Select and lock project runtimes\n  unset        Clear a project or global selection\n  install      Install or repair a runtime\n  paths        Explain Pinset and runtime paths\n  uninstall    Safely uninstall an exact version\n  prune        Remove unused managed versions\n  outdated     Check selected versions for updates\n  current      Show the effective selection\n  list         List installed or available versions\n  lock         Audit lock integrity and ownership\n  cache        Inspect, verify or clean the download cache\n  which        Show the resolved command path\n  exec         Run with the selected version\n  doctor       Diagnose configuration and PATH\n  venv         Manage the project Python environment\n  shim         Repair or migrate command shims\n  env          Manage encrypted project environments\n  trust        Manage local project trust\n  self         Check or update Pinset\n  activate     Enable provider command routing in a shell\n  completions  Generate shell completion\n  source       Manage download sources\n  provider     Inspect and verify Provider manifests\n\nRun `pinset <command> --help` for command details."
            }
            Language::SimplifiedChinese => {
                "Pinset 用于统一管理可复现的运行时版本。\n\n用法：pinset [--lang <en|zh-CN>] <命令>\n\n执行 `pinset <命令> --help` 查看命令详情。"
            }
        }
    }

    pub fn command_help(self, command: Option<&str>) -> &'static str {
        if self.language == Language::English {
            return self.top_level_help();
        }
        match command {
            Some("init") => "创建项目配置。\n\n用法：pinset init",
            Some("detect") => {
                "只读扫描仓库边界内的传统运行时版本配置；不联网、不写文件。\n\n用法：pinset detect [--cwd <目录>] [--json]"
            }
            Some("import") => {
                "将可安全映射的传统运行时版本选择导入 Pinset 配置和锁文件。\n\n用法：pinset import [--cwd <目录>] [--force] [--no-install]"
            }
            Some("global") => {
                "查看或批量设置项目之外使用的全局默认运行时。\n\n用法：pinset global [<工具>@<选择器>...] [--no-install]"
            }
            Some("use") => {
                "批量选择并锁定 Node.js、pnpm、Bun、Go、Python、Flutter、Java、Rust 或 .NET SDK 版本。\n\n用法：pinset use <工具>@<选择器> [<工具>@<选择器>...] [--global] [--no-install]"
            }
            Some("unset") => {
                "清除项目或全局运行时选择，不卸载运行时。\n\n用法：pinset unset <node|pnpm|bun|go|python|flutter|java|rust|dotnet> [--global] [--cwd <目录>]"
            }
            Some("install") => {
                "安装指定运行时版本，或根据项目/全局锁文件安装全部工具；--repair 只修复所有权收据匹配的安装。\n\n用法：\n  pinset install <node|pnpm|bun|go|python|flutter|java|rust|dotnet>@<版本选择器> [--repair]\n  pinset install [--locked] [--global] [--cwd <目录>]"
            }
            Some("paths") => {
                "显示 CLI、shim、数据目录、安装根与可选工具的真实安装路径。\n\n用法：pinset paths [工具] [--json]"
            }
            Some("which") => {
                "显示实际执行的运行时命令路径。\n\n用法：pinset which <命令> [--cwd <目录>] [--json]"
            }
            Some("current") => {
                "显示当前版本、来源和安装路径。\n\n用法：pinset current [node|pnpm|bun|go|python|flutter|java|rust|dotnet] [--cwd <目录>] [--json]"
            }
            Some("list") => {
                "列出本机已安装或官方可用的运行时版本；不传 Provider 时列出全部受管版本。\n\n用法：pinset list [node|pnpm|bun|go|python|flutter|java|rust|dotnet] [--available] [--json]"
            }
            Some("outdated") => {
                "检查当前项目与全局选择是否落后于最新稳定版本。\n\n用法：pinset outdated [工具] [--global|--cwd <目录>] [--json]"
            }
            Some("uninstall") => {
                "卸载 Pinset 管理的精确运行时版本。\n\n用法：pinset uninstall <工具>@<精确版本> [--cwd <目录>] [--force] [--dry-run] [--json]"
            }
            Some("prune") => {
                "清理未被全局、当前项目或显式附加项目选择引用的受管版本。\n\n用法：pinset prune [--cwd <目录>] [--project <目录>...] [--dry-run] [--json]"
            }
            Some("lock") => {
                "只读、离线审计配置、锁定平台制品、缓存、安装收据和所有权。\n\n用法：pinset lock audit [--global | --cwd <目录>] [--json]"
            }
            Some("cache") => {
                "统计、验证、修复、清理或离线导入运行时下载缓存。\n\n用法：pinset cache <list|info|verify|repair|clean|import> [参数...]"
            }
            Some("exec") => {
                "使用当前选择或一次性运行时版本执行命令。\n\n用法：pinset exec [--cwd <目录>] [<工具>@<版本选择器>] -- <命令> [参数...]"
            }
            Some("doctor") => {
                "只读检查配置、锁文件、运行时、shim 和 PATH。\n\n用法：pinset doctor [--cwd <目录>] [--json]"
            }
            Some("venv") => {
                "管理 Pinset 创建并校验归属的项目 Python .venv，无需手动激活。\n\n用法：pinset venv <create|status|recreate> [--cwd <目录>]"
            }
            Some("shim") => {
                "查看、修复或迁移 Pinset Provider 命令路由。\n\n用法：\n  pinset shim path\n  pinset shim install [--provider <工具>] [--binary <文件>] [--dir <目录>] [命令...]\n  pinset shim migrate [--provider <工具>] [--dir <目录>]"
            }
            Some("env") => {
                "管理按 profile 隔离的 age 加密项目环境变量。\n\n用法：pinset env <init|set|unset|list|reveal|import|export|recipient|identity> [参数...]"
            }
            Some("trust") => {
                "管理直接 shim 自动注入所需的本机项目信任。\n\n用法：pinset trust <add|status|revoke> [参数...]"
            }
            Some("self") => {
                "显式检查或安装经过 checksum 验证的 Pinset 版本。\n\n用法：pinset self <outdated|update> [参数...]"
            }
            Some("activate") => {
                "输出启用 Pinset Provider 命令路由的 Shell 脚本。\n\n用法：pinset activate <bash|zsh|fish|powershell>"
            }
            Some("completions") => {
                "生成 Pinset 的 Shell 命令补全脚本。\n\n用法：pinset completions <bash|zsh|fish|powershell>"
            }
            Some("source") => {
                "管理并测试本机下载源。\n\n用法：pinset source <list|add|use|fallback|remove|test> [参数...]"
            }
            Some("provider") => {
                "只读查看并验证受约束的声明式 Provider Registry；验证不会安装、激活或执行第三方代码。\n\n用法：\n  pinset provider list [--json]\n  pinset provider verify [Registry 文件] [--json]"
            }
            _ => {
                "Pinset 用于统一管理可复现的运行时版本。\n\n用法：pinset [--lang <en|zh-CN>] <命令>\n\n命令：\n  init         创建项目配置\n  detect       检测传统版本配置\n  import       导入传统版本选择\n  global       查看或设置全局默认版本\n  use          选择并锁定项目版本\n  unset        清除项目或全局选择\n  install      安装锁定或指定版本\n  uninstall    安全卸载精确版本\n  prune        清理未引用的受管版本\n  outdated     检查已选版本更新\n  current      显示当前生效选择\n  list         列出已安装或可用版本\n  lock         审计锁完整性与所有权\n  cache        统计、验证或清理下载缓存\n  which        显示实际命令路径\n  exec         使用当前选择执行命令\n  doctor       诊断配置与 PATH\n  venv         管理项目 Python 虚拟环境\n  shim         管理和迁移命令 shim\n  activate     为当前 Shell 启用命令路由\n  completions  生成 Shell 命令补全\n  source       管理下载源\n  provider     查看和验证 Provider 清单\n\n执行 `pinset --lang zh-CN <命令> --help` 查看详情。"
            }
        }
    }

    pub fn argument_error(self, kind: &str) -> &'static str {
        if self.language == Language::English {
            return "invalid command arguments";
        }
        match kind {
            "missing" => "错误：缺少必填参数。",
            "unknown" => "错误：存在未知的命令或参数。",
            "invalid" => "错误：参数值无效。",
            "conflict" => "错误：参数之间存在冲突。",
            _ => "错误：命令参数无效。",
        }
    }

    pub fn created(self, path: &Path) -> String {
        match self.language {
            Language::English => format!("created {}", path.display()),
            Language::SimplifiedChinese => format!("已创建 {}", path.display()),
        }
    }

    pub fn selected(self, scope: &str, version: &str, targets: usize, lock_path: &Path) -> String {
        match self.language {
            Language::English => format!(
                "selected {scope} node@{version}; locked {targets} targets in {}",
                lock_path.display()
            ),
            Language::SimplifiedChinese => {
                let scope = if scope == "global" {
                    "全局"
                } else {
                    "项目"
                };
                format!(
                    "已选择{scope} Node.js {version}；已在 {} 中锁定 {targets} 个目标平台",
                    lock_path.display()
                )
            }
        }
    }

    pub fn selection_unset(self, scope: &str, tool: &str, path: &Path, changed: bool) -> String {
        let tool = self.tool_name(tool);
        match self.language {
            Language::English if changed => format!(
                "cleared {scope} {tool} selection in {}; installed runtimes and command routes were preserved",
                path.display()
            ),
            Language::English => format!(
                "no {scope} {tool} selection was configured in {}",
                path.display()
            ),
            Language::SimplifiedChinese if changed => format!(
                "已清除{} {tool} 选择：{}；已安装运行时和命令路由保持不变",
                if scope == "global" {
                    "全局"
                } else {
                    "项目"
                },
                path.display()
            ),
            Language::SimplifiedChinese => format!(
                "{}未配置 {tool} 选择：{}",
                if scope == "global" {
                    "全局"
                } else {
                    "项目"
                },
                path.display()
            ),
        }
    }

    pub fn global_not_selected(self, config_path: &Path) -> String {
        match self.language {
            Language::English => format!(
                "no global Node.js version selected; run `pinset global node@<selector>` (config={})",
                config_path.display()
            ),
            Language::SimplifiedChinese => format!(
                "尚未设置全局 Node.js；请执行 `pinset global node@<版本选择器>`（配置={}）",
                config_path.display()
            ),
        }
    }

    pub fn global_project_override(
        self,
        global_version: &str,
        project_version: &str,
        project_config: &Path,
    ) -> String {
        match self.language {
            Language::English => format!(
                "note: project node@{project_version} overrides global node@{global_version} in this directory (config={})",
                project_config.display()
            ),
            Language::SimplifiedChinese => format!(
                "提示：当前目录由项目 Node.js {project_version} 覆盖全局 Node.js {global_version}（配置={}）",
                project_config.display()
            ),
        }
    }

    pub fn no_installed_node(self) -> &'static str {
        match self.language {
            Language::English => "no Node.js versions are installed by Pinset",
            Language::SimplifiedChinese => "Pinset 尚未安装任何 Node.js 版本",
        }
    }

    pub fn installed_node(self, version: &str, targets: &str) -> String {
        match self.language {
            Language::English => format!("{version} installed targets={targets}"),
            Language::SimplifiedChinese => format!("{version} 已安装 目标={targets}"),
        }
    }

    pub fn available_node(
        self,
        version: &str,
        date: &str,
        lts: Option<&str>,
        security: bool,
    ) -> String {
        let lts = lts.unwrap_or("-");
        match self.language {
            Language::English => {
                format!("{version} available date={date} lts={lts} security={security}")
            }
            Language::SimplifiedChinese => format!(
                "{version} 可用 日期={date} LTS={lts} 安全更新={}",
                if security { "是" } else { "否" }
            ),
        }
    }

    pub fn uninstalled_node(self, version: &str, targets: &str) -> String {
        match self.language {
            Language::English => format!("uninstalled node@{version} targets={targets}"),
            Language::SimplifiedChinese => {
                format!("已卸载 Node.js {version}；目标平台={targets}")
            }
        }
    }

    pub fn cache_empty(self) -> &'static str {
        match self.language {
            Language::English => "the Pinset download cache is empty",
            Language::SimplifiedChinese => "Pinset 下载缓存为空",
        }
    }

    pub fn cache_entry(self, integrity: &str, size: u64, path: &Path) -> String {
        match self.language {
            Language::English => {
                format!("{integrity} cached bytes={size} path={}", path.display())
            }
            Language::SimplifiedChinese => {
                format!("{integrity} 已缓存 字节数={size} 路径={}", path.display())
            }
        }
    }

    pub fn cache_cleaned(self, entries: usize, bytes: u64) -> String {
        match self.language {
            Language::English => format!("cleaned {entries} cached archives ({bytes} bytes)"),
            Language::SimplifiedChinese => {
                format!("已清理 {entries} 个缓存归档（{bytes} 字节）")
            }
        }
    }

    pub fn cache_imported(self, integrity: &str, size: u64, path: &Path) -> String {
        match self.language {
            Language::English => format!(
                "imported verified cache archive integrity={integrity} bytes={size} path={}",
                path.display()
            ),
            Language::SimplifiedChinese => format!(
                "已导入校验通过的缓存归档：完整性={integrity}；字节数={size}；路径={}",
                path.display()
            ),
        }
    }

    pub fn lock_audit_header(
        self,
        scope: &str,
        config: &Path,
        lockfile: &Path,
        passed: bool,
    ) -> String {
        match self.language {
            Language::English => format!(
                "lock audit scope={scope} mode=offline/read-only status={} config={} lock={}",
                if passed { "passed" } else { "action-required" },
                config.display(),
                lockfile.display()
            ),
            Language::SimplifiedChinese => format!(
                "锁审计 范围={} 模式=离线/只读 状态={} 配置={} 锁文件={}",
                if scope == "project" {
                    "项目"
                } else {
                    "全局"
                },
                if passed { "通过" } else { "需要处理" },
                config.display(),
                lockfile.display()
            ),
        }
    }

    pub fn lock_audit_finding(
        self,
        severity: &str,
        reason_code: &str,
        subject: &str,
        path: Option<&Path>,
        message: &str,
    ) -> String {
        let path = path
            .map(|path| format!(" path={}", path.display()))
            .unwrap_or_default();
        match self.language {
            Language::English => {
                format!("[{severity}] {reason_code} subject={subject}{path}: {message}")
            }
            Language::SimplifiedChinese => {
                let severity = match severity {
                    "error" => "错误",
                    "warning" => "警告",
                    _ => "信息",
                };
                format!("[{severity}] {reason_code} 对象={subject}{path}：{message}")
            }
        }
    }

    pub fn lock_audit_repair(self, action: &str, command: Option<&str>) -> String {
        match (self.language, command) {
            (Language::English, Some(command)) => {
                format!("  repair: {action}; command={command}")
            }
            (Language::English, None) => format!("  repair: {action}"),
            (Language::SimplifiedChinese, Some(command)) => {
                format!("  修复计划：{action}；命令={command}")
            }
            (Language::SimplifiedChinese, None) => format!("  修复计划：{action}"),
        }
    }

    pub fn lock_audit_summary(self, summary: &LockAuditSummary) -> String {
        match self.language {
            Language::English => format!(
                "summary tools={} platform_artifacts={} cache_entries={} receipts={} owned_installs={} errors={} warnings={} info={}",
                summary.tools,
                summary.platform_artifacts,
                summary.cache_entries,
                summary.receipts,
                summary.owned_installs,
                summary.errors,
                summary.warnings,
                summary.info
            ),
            Language::SimplifiedChinese => format!(
                "汇总 工具={} 平台制品={} 缓存项={} 收据={} 已确认归属安装={} 错误={} 警告={} 信息={}",
                summary.tools,
                summary.platform_artifacts,
                summary.cache_entries,
                summary.receipts,
                summary.owned_installs,
                summary.errors,
                summary.warnings,
                summary.info
            ),
        }
    }

    pub fn source_test_ok(
        self,
        provider: &str,
        alias: &str,
        base_url: &str,
        releases: usize,
        tls: bool,
    ) -> String {
        match self.language {
            Language::English => format!(
                "source test ok provider={provider} alias={alias} url={base_url} stable_releases={releases} checks=dns,http,index,shasums tls={}",
                if tls {
                    "ok"
                } else {
                    "not-applicable-insecure-http"
                }
            ),
            Language::SimplifiedChinese => format!(
                "安装源测试通过：提供方={provider}；别名={alias}；地址={base_url}；稳定版本数={releases}；检查项=DNS,HTTP,版本索引,SHASUMS；TLS={}",
                if tls {
                    "通过"
                } else {
                    "不适用（显式允许的不安全 HTTP）"
                }
            ),
        }
    }

    pub fn already_installed(self, version: &str, target: &str, path: &Path) -> String {
        match self.language {
            Language::English => format!(
                "already installed node@{version} for {target} at {}",
                path.display()
            ),
            Language::SimplifiedChinese => {
                format!("Node.js {version}（{target}）已安装在 {}", path.display())
            }
        }
    }

    pub fn installed(self, version: &str, target: &str, source: &str, path: &Path) -> String {
        match self.language {
            Language::English => format!(
                "installed node@{version} for {target} from {source} at {}",
                path.display()
            ),
            Language::SimplifiedChinese => format!(
                "已从 {source} 安装 Node.js {version}（{target}）到 {}",
                path.display()
            ),
        }
    }

    pub fn download_started(self, artifact: &str, total: Option<String>) -> String {
        let total = total.unwrap_or_else(|| "unknown size".to_owned());
        match self.language {
            Language::English => format!("downloading {artifact} ({total})"),
            Language::SimplifiedChinese => {
                let total = if total == "unknown size" {
                    "大小未知".to_owned()
                } else {
                    total
                };
                format!("正在下载 {artifact}（{total}）")
            }
        }
    }

    pub fn download_progress(
        self,
        artifact: &str,
        bar: &str,
        percent: u8,
        downloaded: &str,
        total: Option<String>,
    ) -> String {
        match total {
            Some(total) => match self.language {
                Language::English => {
                    format!("downloading {artifact} [{bar}] {percent:>3}% {downloaded}/{total}")
                }
                Language::SimplifiedChinese => {
                    format!("正在下载 {artifact} [{bar}] {percent:>3}% {downloaded}/{total}")
                }
            },
            None => match self.language {
                Language::English => {
                    format!("downloading {artifact} [{bar}] {downloaded}")
                }
                Language::SimplifiedChinese => {
                    format!("正在下载 {artifact} [{bar}] {downloaded}")
                }
            },
        }
    }

    pub fn download_finished(self, artifact: &str, downloaded: &str) -> String {
        match self.language {
            Language::English => {
                format!("downloaded {artifact} ({downloaded}); integrity verified")
            }
            Language::SimplifiedChinese => {
                format!("已下载 {artifact}（{downloaded}）；完整性校验通过")
            }
        }
    }

    pub fn download_failed(self, artifact: &str) -> String {
        match self.language {
            Language::English => format!("download failed: {artifact}"),
            Language::SimplifiedChinese => format!("下载失败：{artifact}"),
        }
    }

    pub fn current_installed(
        self,
        tool: &str,
        version: &str,
        source: &str,
        executable: &Path,
        config: Option<&Path>,
    ) -> String {
        let config = config
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned());
        if source == "system" {
            return match self.language {
                Language::English => format!(
                    "{tool} system installed {} source=system config=-",
                    executable.display()
                ),
                Language::SimplifiedChinese => format!(
                    "系统 PATH 中的 {}：{}；来源=系统 PATH；配置=-",
                    self.tool_name(tool),
                    executable.display()
                ),
            };
        }
        match self.language {
            Language::English => format!(
                "{tool} {version} installed {} source={source} config={config}",
                executable.display()
            ),
            Language::SimplifiedChinese => format!(
                "{} {version} 已安装：{}；来源={}; 配置={config}",
                self.tool_name(tool),
                executable.display(),
                self.source_name(source)
            ),
        }
    }

    pub fn current_missing(
        self,
        tool: &str,
        version: &str,
        source: &str,
        expected: &Path,
        config: Option<&Path>,
    ) -> String {
        let config = config
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned());
        match self.language {
            Language::English => format!(
                "{tool} {version} missing expected={} source={source} config={config}",
                expected.display()
            ),
            Language::SimplifiedChinese => format!(
                "{} {version} 尚未安装；预期路径={}；来源={}; 配置={config}",
                self.tool_name(tool),
                expected.display(),
                self.source_name(source)
            ),
        }
    }

    fn tool_name(self, tool: &str) -> &str {
        if tool == "node" { "Node.js" } else { tool }
    }

    pub fn doctor_line(self, key: &str, value: impl fmt::Display, state: &str) -> String {
        match self.language {
            Language::English => format!("{key} {value} {state}"),
            Language::SimplifiedChinese => {
                format!(
                    "{}：{value} {}",
                    self.doctor_key(key),
                    self.state_name(state)
                )
            }
        }
    }

    pub fn doctor_selection(self, version: &str, source: &str, path: Option<&Path>) -> String {
        let path = path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned());
        if source == "system" {
            return match self.language {
                Language::English => "selection node@unknown source=system path=-".to_owned(),
                Language::SimplifiedChinese => {
                    "当前选择：系统 PATH 中的 Node.js（未执行版本探测）；配置=-".to_owned()
                }
            };
        }
        match self.language {
            Language::English => format!(
                "selection node@{version} source={} path={path}",
                self.source_name(source)
            ),
            Language::SimplifiedChinese => format!(
                "当前选择：Node.js {version}；来源={}；配置={path}",
                self.source_name(source)
            ),
        }
    }

    pub fn doctor_lock_matches(self, path: &Path, version: &str) -> String {
        match self.language {
            Language::English => {
                format!("lockfile {} matches node@{version}", path.display())
            }
            Language::SimplifiedChinese => {
                format!("锁文件：{} 与 Node.js {version} 匹配", path.display())
            }
        }
    }

    pub fn no_selection(self) -> &'static str {
        match self.language {
            Language::English => "selection node missing",
            Language::SimplifiedChinese => "当前选择：未声明 Node.js，系统 PATH 中也未找到",
        }
    }

    pub fn transitional_home_config(self, path: &Path, overrides_global: bool) -> String {
        match self.language {
            Language::English => format!(
                "warning: transitional HOME config {} is inherited by subdirectories{}; migrate manually if this was intended as a global default",
                path.display(),
                if overrides_global {
                    " and overrides global.toml"
                } else {
                    ""
                }
            ),
            Language::SimplifiedChinese => format!(
                "警告：HOME 下的过渡配置 {} 会被子目录继承{}；如果它原本用于全局默认版本，请手动确认后迁移",
                path.display(),
                if overrides_global {
                    "，并会覆盖 global.toml"
                } else {
                    ""
                }
            ),
        }
    }

    pub fn path_candidate(
        self,
        command: &str,
        path: &Path,
        owner: &str,
        effective: bool,
        managed: bool,
    ) -> String {
        match self.language {
            Language::English => format!(
                "path_{command} {} owner={owner} effective={effective} managed={managed}",
                path.display()
            ),
            Language::SimplifiedChinese => format!(
                "PATH 中的 {command}：{}；来源={}；当前生效={}；Pinset 受管={}",
                path.display(),
                match owner {
                    "pinset" => "Pinset shim",
                    "foreign-in-pinset-directory" => "Pinset 目录中的外部文件",
                    _ => "系统或其他工具",
                },
                if effective { "是" } else { "否" },
                if managed { "是" } else { "否" },
            ),
        }
    }

    pub fn doctor_routing_issue(
        self,
        code: &str,
        command: Option<&str>,
        path: Option<&str>,
        action: &str,
    ) -> String {
        match self.language {
            Language::English => format!(
                "routing_issue code={code} command={} path={} action={action}",
                command.unwrap_or("-"),
                path.unwrap_or("-"),
            ),
            Language::SimplifiedChinese => {
                let issue = match code {
                    "routing-directory-not-on-path" => "命令路由目录未加入 PATH",
                    "legacy-shims-present" => "检测到保留的旧 shim",
                    "provider-route-shadowed" => "Pinset 命令路由被更早的 PATH 项遮蔽",
                    "provider-route-conflict" => "目标目录存在外部同名命令",
                    "provider-route-missing" => "Provider 命令路由缺失",
                    "go-toolchain-override" => "显式 GOTOOLCHAIN 可能绕过 Pinset 锁定",
                    "java-classpath-override" => "CLASSPATH 可能影响 Java 类路径解析",
                    "java-tool-options-override" => "JAVA_TOOL_OPTIONS 会自动传入 JVM",
                    "jdk-java-options-override" => "JDK_JAVA_OPTIONS 会自动传入 java 启动器",
                    "java-legacy-options-override" => "_JAVA_OPTIONS 可能被 JVM 自动读取",
                    _ => code,
                };
                format!(
                    "路由问题：{issue}；命令={}；路径={}；建议={action}",
                    command.unwrap_or("-"),
                    path.unwrap_or("-"),
                )
            }
        }
    }

    pub fn source_changed(self, action: &str, provider: &str, value: &str) -> String {
        match self.language {
            Language::English => format!("{action} {provider} {value}"),
            Language::SimplifiedChinese => match action {
                "added" => format!("已为 {provider} 添加安装源 {value}"),
                "active" => format!("{provider} 当前安装源已切换为 {value}"),
                "fallback" if value == "cleared" => format!("已清空 {provider} 的备用安装源"),
                "fallback" => format!("{provider} 的备用安装源已设为 {value}"),
                "removed" => format!("已删除 {provider} 安装源 {value}"),
                _ => format!("{action} {provider} {value}"),
            },
        }
    }

    pub fn shim_installed(self, command: &str, destination: &Path, method: &str) -> String {
        match self.language {
            Language::English => format!("{command} {} {method}", destination.display()),
            Language::SimplifiedChinese => {
                let method = match method {
                    "symbolic-link" => "符号链接",
                    "wrapper" => "命令包装器",
                    "hard-link" => "硬链接",
                    "existing" => "已有受管入口",
                    _ => "复制",
                };
                format!(
                    "已准备命令 {command} 到 {}（{method}）",
                    destination.display()
                )
            }
        }
    }

    pub fn shim_path_ready(self, directory: &Path) -> String {
        match self.language {
            Language::English => format!(
                "shim directory ready: {}; add this directory before other runtime managers in PATH",
                directory.display()
            ),
            Language::SimplifiedChinese => format!(
                "shim 目录已就绪：{}；请将该目录放在 PATH 中其他运行时管理器之前",
                directory.display()
            ),
        }
    }

    pub fn shim_migration_not_needed(self, directory: &Path) -> String {
        match self.language {
            Language::English => format!(
                "command routing already uses {}; no legacy directory migration is needed",
                directory.display()
            ),
            Language::SimplifiedChinese => {
                format!("命令路由已使用 {}；无需迁移旧目录", directory.display())
            }
        }
    }

    pub fn shim_migrated(
        self,
        source: &Path,
        destination: &Path,
        commands: usize,
        preserved: usize,
        active: bool,
    ) -> String {
        match self.language {
            Language::English => format!(
                "registered {commands} command routes in {}; preserved {preserved} legacy entries in {}; destination-on-path={active}",
                destination.display(),
                source.display()
            ),
            Language::SimplifiedChinese => format!(
                "已在 {} 注册 {commands} 个命令路由；{} 中的 {preserved} 个旧入口保持不变；目标目录已加入 PATH={}",
                destination.display(),
                source.display(),
                if active { "是" } else { "否" }
            ),
        }
    }

    pub fn provider_commands_registered(
        self,
        provider: &str,
        directory: &Path,
        installed: &[&str],
        preserved: &[&str],
        routing: Option<(&[String], &str)>,
    ) -> String {
        let (active, shadowed, activation_command) = match routing {
            Some((shadowed, activation_command)) => (false, shadowed, activation_command),
            None => (true, &[] as &[String], ""),
        };
        let installed = if installed.is_empty() {
            "-".to_owned()
        } else {
            installed.join(",")
        };
        let preserved = if preserved.is_empty() {
            "-".to_owned()
        } else {
            preserved.join(",")
        };
        match self.language {
            Language::English => {
                let activation = if active {
                    String::new()
                } else if shadowed.is_empty() {
                    format!("; run `{activation_command}` in the current shell")
                } else {
                    format!(
                        "; warning: earlier PATH entries shadow {}; run `{activation_command}` in the current shell",
                        shadowed.join(",")
                    )
                };
                format!(
                    "{provider} command routing ready: {} (created={installed}; managed-existing={preserved}){activation}",
                    directory.display()
                )
            }
            Language::SimplifiedChinese => {
                let activation = if active {
                    String::new()
                } else if shadowed.is_empty() {
                    format!("；当前 Shell 请执行 `{activation_command}`")
                } else {
                    format!(
                        "；警告：更早的 PATH 命令遮挡了 {}；当前 Shell 请执行 `{activation_command}`",
                        shadowed.join(",")
                    )
                };
                format!(
                    "{provider} 命令路由已就绪：{}（已创建={installed}；已有受管命令={preserved}）{activation}",
                    directory.display()
                )
            }
        }
    }

    pub fn shim_auto_registration_failed(self, reason: &str) -> String {
        match self.language {
            Language::English => format!(
                "warning: runtime installation succeeded, but provider command routing could not be prepared: {reason}"
            ),
            Language::SimplifiedChinese => {
                format!("警告：运行时安装成功，但无法准备 Provider 命令路由：{reason}")
            }
        }
    }

    pub fn selection_error(self) -> &'static str {
        match self.language {
            Language::English => "selection must use <tool>@<selector>",
            Language::SimplifiedChinese => "版本选择必须使用 <工具>@<选择器> 格式",
        }
    }

    pub fn utf8_command_error(self) -> &'static str {
        match self.language {
            Language::English => "command name must be valid UTF-8",
            Language::SimplifiedChinese => "命令名称必须是有效的 UTF-8 文本",
        }
    }

    fn source_name(self, source: &str) -> &str {
        if self.language == Language::English {
            return source;
        }
        match source {
            "project" => "项目",
            "global" => "全局",
            "system" => "系统 PATH",
            _ => source,
        }
    }

    fn doctor_key(self, key: &str) -> &str {
        match key {
            "pinset_home" => "Pinset 数据目录",
            "project_config" => "项目配置",
            "global_config" => "全局配置",
            "selection" => "当前选择",
            "lockfile" => "锁文件",
            "runtime" => "运行时",
            "python_environment" => "Python 项目虚拟环境",
            "shim_path" => "Shim 路径",
            "path_node" => "PATH 中的 Node",
            _ => key,
        }
    }

    fn state_name(self, state: &str) -> &str {
        match state {
            "ok" => "正常",
            "missing" => "缺失",
            "invalid" => "无效",
            "active" => "已启用",
            "not-on-path" => "未加入 PATH",
            _ => state,
        }
    }
}
