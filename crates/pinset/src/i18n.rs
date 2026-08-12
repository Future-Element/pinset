use std::{error::Error as StdError, fmt, path::Path, str::FromStr};

use pinset_core::Error;

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
                format!("错误：不支持 pinset.toml schema {actual}，当前仅支持 schema 1")
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
                format!("错误：不支持 global.toml schema {actual}，当前仅支持 schema 1")
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
                "Pinset manages predictable runtime versions.\n\nUsage: pinset [--lang <en|zh-CN>] <COMMAND>\n\nRun `pinset <command> --help` for command details."
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
            Some("use") => {
                "选择并锁定 Node.js 版本。\n\n用法：pinset use <node@x.y.z> [--global] [--no-install]"
            }
            Some("install") => {
                "根据项目或全局锁文件安装当前平台运行时。\n\n用法：pinset install [--locked] [--global] [--cwd <目录>]"
            }
            Some("which") => {
                "显示实际执行的运行时命令路径。\n\n用法：pinset which <命令> [--cwd <目录>]"
            }
            Some("current") => {
                "显示当前版本、来源和安装路径。\n\n用法：pinset current [node] [--cwd <目录>]"
            }
            Some("exec") => {
                "使用当前选择执行命令。\n\n用法：pinset exec [--cwd <目录>] -- <命令> [参数...]"
            }
            Some("doctor") => {
                "只读检查配置、锁文件、运行时、shim 和 PATH。\n\n用法：pinset doctor [--cwd <目录>]"
            }
            Some("shim") => {
                "管理 Pinset 多调用 shim。\n\n用法：pinset shim install --binary <文件> --dir <目录> [命令...]"
            }
            Some("source") => {
                "管理本机下载源。\n\n用法：pinset source <list|add|use|fallback|remove> [参数...]"
            }
            _ => {
                "Pinset 用于统一管理可复现的运行时版本。\n\n用法：pinset [--lang <en|zh-CN>] <命令>\n\n命令：\n  init      创建项目配置\n  use       选择并锁定版本\n  install   安装锁定版本\n  current   显示当前选择\n  which     显示实际命令路径\n  exec      使用当前选择执行命令\n  doctor    诊断配置与 PATH\n  shim      管理命令 shim\n  source    管理下载源\n\n执行 `pinset --lang zh-CN <命令> --help` 查看详情。"
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

    pub fn current_installed(
        self,
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
                    "node system installed {} source=system config=-",
                    executable.display()
                ),
                Language::SimplifiedChinese => format!(
                    "系统 PATH 中的 Node.js：{}；来源=系统 PATH；配置=-",
                    executable.display()
                ),
            };
        }
        match self.language {
            Language::English => format!(
                "node {version} installed {} source={source} config={config}",
                executable.display()
            ),
            Language::SimplifiedChinese => format!(
                "Node.js {version} 已安装：{}；来源={}; 配置={config}",
                executable.display(),
                self.source_name(source)
            ),
        }
    }

    pub fn current_missing(
        self,
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
                "node {version} missing expected={} source={source} config={config}",
                expected.display()
            ),
            Language::SimplifiedChinese => format!(
                "Node.js {version} 尚未安装；预期路径={}；来源={}; 配置={config}",
                expected.display(),
                self.source_name(source)
            ),
        }
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

    pub fn path_candidate(self, path: &Path, owner: &str) -> String {
        match self.language {
            Language::English => {
                format!("path_node {} owner={owner}", path.display())
            }
            Language::SimplifiedChinese => format!(
                "PATH 中的 Node：{}；来源={}",
                path.display(),
                match owner {
                    "pinset" => "Pinset shim",
                    "nvm" => "nvm",
                    "fnm" => "fnm",
                    "asdf" => "asdf",
                    "mise" => "mise",
                    "volta" => "Volta",
                    _ => "系统或其他工具",
                }
            ),
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
            Language::SimplifiedChinese => format!(
                "已安装命令 {command} 到 {}（{}）",
                destination.display(),
                if method == "hard-link" {
                    "硬链接"
                } else {
                    "复制"
                }
            ),
        }
    }

    pub fn selection_error(self) -> &'static str {
        match self.language {
            Language::English => "selection must use node@x.y.z",
            Language::SimplifiedChinese => "版本选择必须使用 node@x.y.z 格式",
        }
    }

    pub fn node_only_error(self) -> &'static str {
        match self.language {
            Language::English => "the Node-first MVP only accepts node@x.y.z",
            Language::SimplifiedChinese => "当前 Node-first 版本只接受 node@x.y.z",
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
            "shim_path" => "Shim 路径",
            "path_node" => "PATH 中的 Node",
            _ => key,
        }
    }

    fn state_name(self, state: &str) -> &str {
        match state {
            "ok" => "正常",
            "missing" => "缺失",
            "active" => "已启用",
            "not-on-path" => "未加入 PATH",
            _ => state,
        }
    }
}
