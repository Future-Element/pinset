//! Transient stderr UI. Engine events never alter machine-readable reports.
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use pinset_engine::{DownloadProgressEvent, InstallPhase, ProgressEvent};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct ProgressDisplay(Arc<Mutex<DisplayState>>);
struct DisplayState {
    enabled: bool,
    chinese: bool,
    prefix: String,
    bar: Option<ProgressBar>,
}
fn clean(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).collect()
}
fn filename(url: &str) -> String {
    clean(url.rsplit('/').next().unwrap_or("artifact"))
}
impl ProgressDisplay {
    pub fn new(enabled: bool, chinese: bool) -> Self {
        Self(Arc::new(Mutex::new(DisplayState {
            enabled,
            chinese,
            prefix: String::new(),
            bar: None,
        })))
    }
    pub fn reporter(&self) -> impl Fn(ProgressEvent) + Send + Sync + 'static {
        let state = Arc::clone(&self.0);
        move |event| {
            if let Ok(mut state) = state.lock()
                && state.enabled
            {
                state.report(event);
            }
        }
    }
}
impl Drop for ProgressDisplay {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.clear();
        }
    }
}
impl DisplayState {
    fn text<'a>(&self, english: &'a str, chinese: &'a str) -> &'a str {
        if self.chinese { chinese } else { english }
    }
    fn clear(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }
    fn spinner(&mut self, message: String) {
        self.clear();
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::hidden()).with_style(
            ProgressStyle::with_template("{spinner:.cyan} {prefix} {msg} [{elapsed_precise}]")
                .expect("static progress template"),
        );
        bar.set_prefix(self.prefix.clone());
        bar.set_message(message);
        bar.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
        bar.enable_steady_tick(Duration::from_millis(100));
        self.bar = Some(bar);
    }
    fn note(&mut self, message: String) {
        self.clear();
        eprintln!("{} {}", self.prefix, message);
    }
    fn report(&mut self, event: ProgressEvent) {
        match event {
            ProgressEvent::Resolving {
                tool,
                selector,
                index,
                total,
            } => {
                self.prefix = format!("[{index}/{total}] {}@{}", clean(&tool), clean(&selector));
                self.spinner(
                    self.text("Resolving official version", "解析官方版本")
                        .into(),
                );
            }
            ProgressEvent::Resolved { tool, version } => self.note(format!(
                "{} {}@{}",
                self.text("Resolved", "已解析"),
                clean(&tool),
                clean(&version)
            )),
            ProgressEvent::Installing {
                tool,
                version,
                index,
                total,
            } => {
                self.prefix = format!("[{index}/{total}] {}@{}", clean(&tool), clean(&version));
                self.spinner(self.text("Preparing installation", "准备安装").into());
            }
            ProgressEvent::Download(DownloadProgressEvent::Started { url, total_bytes }) => {
                self.clear();
                let template = if total_bytes.is_some() {
                    "{spinner:.cyan} {prefix} {msg} [{bar:24.cyan/blue}] {percent:>3}% {bytes}/{total_bytes} {bytes_per_sec} ETA {eta}"
                } else {
                    "{spinner:.cyan} {prefix} {msg} {bytes} {bytes_per_sec} [{elapsed_precise}]"
                };
                let bar = ProgressBar::with_draw_target(total_bytes, ProgressDrawTarget::hidden())
                    .with_style(
                        ProgressStyle::with_template(template)
                            .expect("static download template")
                            .progress_chars("=>-"),
                    );
                bar.set_prefix(self.prefix.clone());
                bar.set_message(format!(
                    "{} {}",
                    self.text("Downloading", "下载"),
                    filename(&url)
                ));
                bar.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
                bar.enable_steady_tick(Duration::from_millis(100));
                self.bar = Some(bar);
            }
            ProgressEvent::Download(DownloadProgressEvent::Advanced {
                downloaded_bytes,
                total_bytes,
            }) => {
                if let Some(bar) = &self.bar {
                    if let Some(total) = total_bytes {
                        bar.set_length(total);
                    }
                    bar.set_position(downloaded_bytes);
                }
            }
            ProgressEvent::Download(
                DownloadProgressEvent::Finished { .. } | DownloadProgressEvent::Failed,
            ) => self.clear(),
            ProgressEvent::Phase(phase) => {
                let (en, zh) = match phase {
                    InstallPhase::Waiting => ("Waiting for install lock", "等待安装锁"),
                    InstallPhase::Connecting => ("Connecting to official source", "连接官方来源"),
                    InstallPhase::Verifying => ("Verifying artifact checksum", "校验制品摘要"),
                    InstallPhase::Extracting => ("Extracting verified artifact", "解压已校验制品"),
                    InstallPhase::CheckingInstallation => {
                        ("Checking installation contents", "检查安装内容")
                    }
                    InstallPhase::Binding => (
                        "Binding command entries and project environment",
                        "绑定命令入口与项目环境",
                    ),
                    InstallPhase::Committing => {
                        ("Saving configuration and exact lock", "保存配置与精确锁")
                    }
                };
                if matches!(phase, InstallPhase::Binding | InstallPhase::Committing) {
                    self.prefix.clear();
                }
                self.spinner(self.text(en, zh).into());
            }
            ProgressEvent::Cached { url } => self.note(format!(
                "{} {}",
                self.text("Verified cache", "已校验缓存"),
                filename(&url)
            )),
            ProgressEvent::Installed { reused } => self.note(
                self.text(
                    if reused {
                        "Already installed; reused"
                    } else {
                        "Installed"
                    },
                    if reused {
                        "已安装，直接复用"
                    } else {
                        "安装完成"
                    },
                )
                .into(),
            ),
        }
    }
}
