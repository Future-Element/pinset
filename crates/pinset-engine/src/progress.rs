//! Optional operation events; terminal rendering belongs to the CLI.
use crate::DownloadProgressEvent;
use std::{fmt, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    Waiting,
    Connecting,
    Verifying,
    Extracting,
    CheckingInstallation,
    Binding,
    Committing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    Resolving {
        tool: String,
        selector: String,
        index: usize,
        total: usize,
    },
    Resolved {
        tool: String,
        version: String,
    },
    Installing {
        tool: String,
        version: String,
        index: usize,
        total: usize,
    },
    Download(DownloadProgressEvent),
    Phase(InstallPhase),
    Cached {
        url: String,
    },
    Installed {
        reused: bool,
    },
}

#[derive(Clone, Default)]
pub struct ProgressReporter(Option<Arc<dyn Fn(ProgressEvent) + Send + Sync>>);
impl fmt::Debug for ProgressReporter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProgressReporter")
            .field(&self.0.is_some())
            .finish()
    }
}
impl ProgressReporter {
    pub fn new(reporter: impl Fn(ProgressEvent) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(reporter)))
    }
    pub fn report(&self, event: ProgressEvent) {
        if let Some(reporter) = &self.0 {
            reporter(event);
        }
    }
}
