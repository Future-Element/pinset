//! Secret-free environment and execution evidence shared by CLI and editor consumers.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessState {
    Pass,
    Fail,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentCheck {
    pub id: String,
    pub state: ReadinessState,
    pub reason: String,
    pub next_step: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDescriptor {
    pub tool: String,
    pub requested: String,
    pub locked_version: Option<String>,
    pub installation_identity: Option<String>,
    pub selection_source: String,
    pub target: String,
    pub commands: Vec<String>,
    /// Present only in a local report. Never included in portable exports.
    pub executable: Option<String>,
    pub checks: Vec<EnvironmentCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEvidence {
    pub entry: String,
    pub tool: String,
    pub state: ReadinessState,
    pub expected_executable: Option<String>,
    pub observed_executable: Option<String>,
    pub observed_version: Option<String>,
    pub observed_unix_ms: u64,
    pub context_fingerprint: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentDescriptor {
    pub schema: u32,
    pub cli_version: String,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub target: String,
    pub host: String,
    pub profile: Option<String>,
    pub profile_source: String,
    pub context_fingerprint: Option<String>,
    pub runtimes: Vec<RuntimeDescriptor>,
    pub checks: Vec<EnvironmentCheck>,
    pub evidence: Vec<ExecutionEvidence>,
    pub environment_ready: bool,
    pub execution_verified: bool,
}

impl EnvironmentDescriptor {
    /// Portable reports contain no machine-local paths or contextual fingerprints.
    pub fn portable(&self) -> Self {
        let mut report = self.clone();
        report.project_root = None;
        report.context_fingerprint = None;
        for runtime in &mut report.runtimes {
            runtime.executable = None;
        }
        for evidence in &mut report.evidence {
            evidence.expected_executable = None;
            evidence.observed_executable = None;
            evidence.context_fingerprint = None;
        }
        report
    }

    pub fn update_readiness(&mut self) {
        self.environment_ready = !self.runtimes.is_empty()
            && self
                .checks
                .iter()
                .chain(self.runtimes.iter().flat_map(|runtime| &runtime.checks))
                .all(|check| {
                    matches!(
                        check.state,
                        ReadinessState::Pass | ReadinessState::NotApplicable
                    )
                });
        // This flag describes the requested evidence set, never unobserved IDE entry points.
        self.execution_verified = !self.evidence.is_empty()
            && self
                .evidence
                .iter()
                .all(|evidence| evidence.state == ReadinessState::Pass);
    }
}
