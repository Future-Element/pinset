//! Transient stderr UI. Engine events never alter machine-readable reports.
use console::{Term, truncate_str};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle, TermLike};
use pinset_engine::{DownloadProgressEvent, InstallPhase, ProgressEvent};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct ProgressDisplay(Arc<Mutex<DisplayState>>);
struct DisplayState {
    enabled: bool,
    chinese: bool,
    prefix: String,
    bar: Option<ProgressBar>,
    download_width: u16,
    download_filename: String,
}

/// A progress frame owns one physical row, even if the window shrinks between
/// measuring its width and writing it. indicatif's normal target counts wrapped
/// rows using the old width; queued output can then wrap at the new width and
/// leave rows outside that count. Give it a single logical canvas and clip the
/// complete frame at flush instead. Disable wrapping during that one write so
/// a concurrent resize cannot create a second row.
#[derive(Debug)]
struct SingleLineTerminal {
    term: Term,
    frame: Mutex<String>,
}
impl SingleLineTerminal {
    fn target() -> ProgressDrawTarget {
        let term = Term::buffered_stderr();
        if !term.is_term() || console::is_dumb() {
            return ProgressDrawTarget::hidden();
        }
        ProgressDrawTarget::term_like_with_hz(
            Box::new(Self {
                term,
                frame: Mutex::new(String::new()),
            }),
            10,
        )
    }
}
impl Drop for SingleLineTerminal {
    fn drop(&mut self) {
        // A failed/partial terminal write may not have reached the restoration
        // bytes. Restore wrapping again without clearing permanent messages.
        let _ = self.term.write_str("\x1b[?7h");
        let _ = self.term.flush();
    }
}
impl TermLike for SingleLineTerminal {
    fn width(&self) -> u16 {
        u16::MAX
    }
    fn height(&self) -> u16 {
        1
    }
    fn move_cursor_up(&self, _: usize) -> io::Result<()> {
        Ok(())
    }
    fn move_cursor_down(&self, _: usize) -> io::Result<()> {
        Ok(())
    }
    fn move_cursor_right(&self, _: usize) -> io::Result<()> {
        Ok(())
    }
    fn move_cursor_left(&self, _: usize) -> io::Result<()> {
        Ok(())
    }
    fn write_line(&self, value: &str) -> io::Result<()> {
        self.write_str(value)
    }
    fn write_str(&self, value: &str) -> io::Result<()> {
        // indicatif pads the virtual canvas to its width. The line is erased
        // before drawing, so no padding (or cursor movement) is necessary.
        if !value.trim().is_empty() {
            self.frame.lock().unwrap().push_str(value);
        }
        Ok(())
    }
    fn clear_line(&self) -> io::Result<()> {
        self.frame.lock().unwrap().clear();
        Ok(())
    }
    fn flush(&self) -> io::Result<()> {
        let frame = self.frame.lock().unwrap();
        let width = usize::from(self.term.size().1.saturating_sub(1));
        let clipped = truncate_str(&frame, width, "");
        // Keep restoration in the same buffered write on success and failure.
        self.term
            .write_str(&format!("\r\x1b[2K\x1b[?7l{clipped}\x1b[?7h\r"))?;
        self.term.flush()
    }
}

fn download_style(known: bool, width: u16) -> ProgressStyle {
    // Progress remains visible in narrow windows; filenames and estimates are
    // supplementary. The draw target still clips old, queued wider frames.
    let template = match (known, width) {
        (true, 0..=39) => "{percent}% {bytes} {msg}",
        (true, 40..=59) => "{spinner:.cyan} {percent:>3}% {bytes}/{total_bytes} {msg} {prefix}",
        (true, 60..=79) => {
            "{spinner:.cyan} {percent:>3}% {bytes}/{total_bytes} {bytes_per_sec} {msg} {prefix}"
        }
        (true, 80..=119) => {
            "{spinner:.cyan} {percent:>3}% {bytes}/{total_bytes} {bytes_per_sec} {prefix} {msg}"
        }
        (true, _) => {
            "{spinner:.cyan} {percent:>3}% {bytes}/{total_bytes} {bytes_per_sec} ETA {eta} {prefix} {msg} [{bar:24.cyan/blue}]"
        }
        (false, 0..=39) => "{spinner:.cyan} {bytes} {msg}",
        (false, 40..=79) => "{spinner:.cyan} {bytes} {bytes_per_sec} {msg} {prefix}",
        (false, _) => "{spinner:.cyan} {bytes} {bytes_per_sec} {prefix} {msg} [{elapsed_precise}]",
    };
    ProgressStyle::with_template(template)
        .expect("static download template")
        .progress_chars("=>-")
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
            download_width: 0,
            download_filename: String::new(),
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
        bar.set_draw_target(SingleLineTerminal::target());
        bar.enable_steady_tick(Duration::from_millis(100));
        self.bar = Some(bar);
    }
    fn note(&mut self, message: String) {
        self.clear();
        eprintln!("{} {}", self.prefix, message);
    }
    fn download_message(&self) -> String {
        let label = self.text("Downloading", "下载");
        if self.download_width < 80 {
            label.into()
        } else {
            format!("{label} {}", self.download_filename)
        }
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
                self.download_width = Term::stderr().size().1;
                self.download_filename = filename(&url);
                let bar = ProgressBar::with_draw_target(total_bytes, ProgressDrawTarget::hidden())
                    .with_style(download_style(total_bytes.is_some(), self.download_width));
                bar.set_prefix(self.prefix.clone());
                bar.set_message(self.download_message());
                bar.set_draw_target(SingleLineTerminal::target());
                bar.enable_steady_tick(Duration::from_millis(100));
                self.bar = Some(bar);
            }
            ProgressEvent::Download(DownloadProgressEvent::Advanced {
                downloaded_bytes,
                total_bytes,
            }) => {
                if let Some(bar) = &self.bar {
                    let width = Term::stderr().size().1;
                    if width != self.download_width {
                        self.download_width = width;
                        bar.set_style(download_style(total_bytes.is_some(), width));
                        bar.set_message(self.download_message());
                    }
                    if let Some(total) = total_bytes {
                        bar.set_length(total);
                    }
                    bar.set_position(downloaded_bytes);
                }
            }
            ProgressEvent::Download(DownloadProgressEvent::Retrying {
                attempt,
                max_attempts,
                downloaded_bytes,
                delay,
            }) => {
                self.note(format!(
                    "{} {attempt}/{max_attempts}; {} {:.2} MiB; {} {:.2}s",
                    self.text("Download interrupted; retrying", "下载中断，重试"),
                    self.text("resume at", "从已下载位置继续"),
                    downloaded_bytes as f64 / 1_048_576.0,
                    self.text("wait", "等待"),
                    delay.as_secs_f64()
                ));
                self.spinner(
                    self.text("Waiting to retry official download", "等待重试官方下载")
                        .into(),
                );
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
