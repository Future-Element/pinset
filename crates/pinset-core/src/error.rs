use std::{env::JoinPathsError, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no pinset.toml was found from {start} or its ancestors")]
    ProjectConfigNotFound { start: PathBuf },

    #[error("failed to read project config {path}: {source}")]
    ReadProjectConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("project config {path} is invalid: {source}")]
    ParseProjectConfig {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("unsupported pinset.toml schema {actual}; this spike supports schema 1")]
    UnsupportedSchema { actual: u32 },

    #[error("global config does not exist: {path}")]
    GlobalConfigNotFound { path: PathBuf },

    #[error("failed to read global config {path}: {source}")]
    ReadGlobalConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("global config {path} is invalid: {source}")]
    ParseGlobalConfig {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("unsupported global.toml schema {actual}; this version supports schema 1")]
    UnsupportedGlobalConfigSchema { actual: u32 },

    #[error("failed to read user settings {path}: {source}")]
    ReadUserSettings {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("user settings {path} are invalid: {source}")]
    ParseUserSettings {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("unsupported settings.toml schema {actual}; this version supports schema 1")]
    UnsupportedUserSettingsSchema { actual: u32 },

    #[cfg(feature = "state-write")]
    #[error("failed to serialize user settings: {source}")]
    SerializeUserSettings {
        #[source]
        source: toml::ser::Error,
    },

    #[cfg(feature = "state-write")]
    #[error("failed to create user settings directory {path}: {source}")]
    CreateUserSettingsDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "state-write")]
    #[error("failed to atomically write user settings {path}: {source}")]
    WriteUserSettings {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "state-write")]
    #[error("failed to serialize global config: {source}")]
    SerializeGlobalConfig {
        #[source]
        source: toml::ser::Error,
    },

    #[cfg(feature = "state-write")]
    #[error("failed to create global state directory {path}: {source}")]
    CreateGlobalStateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "state-write")]
    #[error("failed to atomically write global config {path}: {source}")]
    WriteGlobalConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "lockfile")]
    #[error("failed to read lockfile {path}: {source}")]
    ReadLockfile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "lockfile")]
    #[error("lockfile {path} is invalid TOML: {source}")]
    ParseLockfile {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[cfg(feature = "lockfile")]
    #[error("unsupported pinset.lock schema {actual}; this version supports schema 1")]
    UnsupportedLockfileSchema { actual: u32 },

    #[cfg(feature = "lockfile")]
    #[error("invalid pinset.lock: {reason}")]
    InvalidLockfile { reason: String },

    #[cfg(feature = "lockfile")]
    #[error("failed to serialize lockfile: {source}")]
    SerializeLockfile {
        #[source]
        source: toml::ser::Error,
    },

    #[cfg(feature = "lockfile")]
    #[error("failed to atomically write lockfile {path}: {source}")]
    WriteLockfile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "lockfile")]
    #[error("pinset.lock does not contain tool \"{tool}\"")]
    LockedToolMissing { tool: String },

    #[cfg(feature = "lockfile")]
    #[error("pinset.lock does not contain {tool} artifact for {target}")]
    LockedArtifactMissing { tool: String, target: String },

    #[cfg(feature = "lockfile")]
    #[error(
        "{selection_path} selects {tool}@{configured}, but its lockfile contains {tool}@{locked}; regenerate the matching lockfile"
    )]
    LockfileMismatch {
        selection_path: PathBuf,
        tool: String,
        configured: String,
        locked: String,
    },

    #[cfg(feature = "project-write")]
    #[error("refusing to overwrite existing project config: {path}")]
    ProjectConfigAlreadyExists { path: PathBuf },

    #[cfg(feature = "project-write")]
    #[error("failed to serialize project config: {source}")]
    SerializeProjectConfig {
        #[source]
        source: toml::ser::Error,
    },

    #[cfg(feature = "project-write")]
    #[error("failed to atomically create project config {path}: {source}")]
    WriteProjectConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("command \"{command}\" is not handled by the Spike A shim")]
    UnsupportedCommand { command: String },

    #[error("tool \"{tool}\" is not declared in {config_path}")]
    ToolNotConfigured { tool: String, config_path: PathBuf },

    #[error(
        "no version selected for tool \"{tool}\"; checked project ancestors from {start} and global config {global_config_path}"
    )]
    ToolSelectionNotFound {
        tool: String,
        start: PathBuf,
        global_config_path: PathBuf,
    },

    #[error(
        "no project, global or system PATH command was found for \"{command}\"; searched PATH entries: {searched}"
    )]
    CommandSelectionNotFound { command: String, searched: String },

    #[error(
        "runtime command \"{command}\" for {tool}@{version} is not installed; searched: {searched}"
    )]
    RuntimeCommandNotFound {
        tool: String,
        version: String,
        command: String,
        searched: String,
    },

    #[error("runtime executable has no parent directory: {path}")]
    RuntimeCommandDirectoryMissing { path: PathBuf },

    #[error("failed to construct PATH for the selected runtime: {source}")]
    RuntimePathJoin {
        #[source]
        source: JoinPathsError,
    },

    #[error("cannot determine Pinset data directory; set PINSET_HOME explicitly")]
    PinsetHomeUnavailable,

    #[cfg(feature = "sources")]
    #[error("unsupported source provider \"{provider}\"; expected one of: node, python, flutter")]
    UnsupportedSourceProvider { provider: String },

    #[cfg(feature = "sources")]
    #[error("invalid source alias \"{alias}\"; use lowercase letters, digits, '.', '_' or '-'")]
    InvalidSourceAlias { alias: String },

    #[cfg(feature = "sources")]
    #[error("source alias \"official\" is built in and cannot be added, changed or removed")]
    BuiltinSourceMutation,

    #[cfg(feature = "sources")]
    #[error("source \"{alias}\" already exists for provider \"{provider}\"")]
    SourceAlreadyExists { provider: String, alias: String },

    #[cfg(feature = "sources")]
    #[error("source \"{alias}\" does not exist for provider \"{provider}\"")]
    SourceNotFound { provider: String, alias: String },

    #[cfg(feature = "sources")]
    #[error("source \"{alias}\" is currently {usage} for provider \"{provider}\"")]
    SourceInUse {
        provider: String,
        alias: String,
        usage: &'static str,
    },

    #[cfg(feature = "sources")]
    #[error("invalid base URL \"{url}\": {reason}")]
    InvalidSourceBaseUrl { url: String, reason: String },

    #[cfg(feature = "sources")]
    #[error("source fallback for \"{provider}\" contains duplicate alias \"{alias}\"")]
    DuplicateSourceFallback { provider: String, alias: String },

    #[cfg(feature = "sources")]
    #[error("source fallback for \"{provider}\" cannot contain active source \"{alias}\"")]
    ActiveSourceInFallback { provider: String, alias: String },

    #[cfg(feature = "sources")]
    #[error("invalid provider artifact path \"{path}\"; expected a safe relative URL path")]
    InvalidSourceArtifactPath { path: String },

    #[cfg(feature = "sources")]
    #[error("failed to join source \"{alias}\" base URL with artifact path \"{path}\": {source}")]
    JoinSourceArtifactUrl {
        alias: String,
        path: String,
        #[source]
        source: url::ParseError,
    },

    #[cfg(feature = "sources")]
    #[error("failed to read source config {path}: {source}")]
    ReadSourceConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "sources")]
    #[error("source config {path} is invalid: {source}")]
    ParseSourceConfig {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[cfg(feature = "sources")]
    #[error("unsupported sources.toml schema {actual}; this version supports schema 1")]
    UnsupportedSourceSchema { actual: u32 },

    #[cfg(feature = "sources")]
    #[error("failed to serialize source config: {source}")]
    SerializeSourceConfig {
        #[source]
        source: toml::ser::Error,
    },

    #[cfg(feature = "sources")]
    #[error("failed to create source config directory {path}: {source}")]
    CreateSourceConfigDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "sources")]
    #[error("failed to atomically write source config {path}: {source}")]
    WriteSourceConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "node-provider")]
    #[error("invalid exact Node.js version \"{version}\"; expected x.y.z without a leading 'v'")]
    InvalidNodeVersion { version: String },

    #[cfg(feature = "node-provider")]
    #[error(
        "unsupported Node.js target \"{target}\"; expected windows/linux/macos with x86_64/aarch64"
    )]
    UnsupportedNodeTarget { target: String },

    #[cfg(feature = "node-metadata")]
    #[error("failed to request official Node.js metadata {url}: {source}")]
    NodeMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "node-metadata")]
    #[error("failed while reading official Node.js metadata {url}: {source}")]
    NodeMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "node-metadata")]
    #[error("official Node.js SHASUMS exceeds {limit} bytes")]
    NodeMetadataTooLarge { limit: u64 },

    #[cfg(feature = "node-metadata")]
    #[error("invalid official Node.js SHASUMS: {reason}")]
    InvalidNodeShasums { reason: String },

    #[cfg(feature = "node-metadata")]
    #[error("Node.js {version} SHASUMS does not contain {filename}")]
    NodeChecksumMissing { version: String, filename: String },

    #[error("failed to create shim directory {path}: {source}")]
    CreateShimDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to install shim {destination} from {source_path}: {source}")]
    InstallShim {
        source_path: PathBuf,
        destination: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("shim source does not exist or is not a file: {path}")]
    InvalidShimSource { path: PathBuf },

    #[error("invalid shim command name \"{command}\"")]
    InvalidShimCommand { command: String },

    #[error("duplicate shim command name \"{command}\"")]
    DuplicateShimCommand { command: String },

    #[error("refusing to overwrite existing shim destination: {path}")]
    ShimDestinationExists { path: PathBuf },

    #[cfg(feature = "installer")]
    #[error("invalid install path segment for {field}: \"{value}\"")]
    InvalidInstallSegment { field: &'static str, value: String },

    #[cfg(feature = "installer")]
    #[error("invalid archive strip-components value {value}; maximum is 8")]
    InvalidStripComponents { value: usize },

    #[cfg(feature = "installer")]
    #[error("an install request must declare at least one required runtime path")]
    RequiredPathsEmpty,

    #[cfg(feature = "installer")]
    #[error("an artifact must declare at least one download source")]
    ArtifactSourcesEmpty,

    #[cfg(feature = "installer")]
    #[error("invalid artifact source id \"{value}\"")]
    InvalidArtifactSourceId { value: String },

    #[cfg(feature = "installer")]
    #[error("duplicate artifact source id \"{value}\"")]
    DuplicateArtifactSourceId { value: String },

    #[cfg(feature = "installer")]
    #[error("all artifact sources failed ({attempted}); last error: {last_error}")]
    ArtifactSourcesExhausted {
        attempted: String,
        last_error: String,
    },

    #[cfg(feature = "installer")]
    #[error("required runtime path must be relative and contained: {path}")]
    InvalidRequiredPath { path: PathBuf },

    #[cfg(feature = "installer")]
    #[error("invalid SHA-256 value \"{value}\"; expected 64 hexadecimal characters")]
    InvalidSha256 { value: String },

    #[cfg(feature = "installer")]
    #[error("failed to build the HTTP client: {source}")]
    HttpClient {
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "installer")]
    #[error("failed to request artifact {url}: {source}")]
    DownloadRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "installer")]
    #[error("failed while reading artifact {url}: {source}")]
    DownloadRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("artifact from {url} exceeds download limit {limit} bytes")]
    DownloadTooLarge { url: String, limit: u64 },

    #[cfg(feature = "installer")]
    #[error("failed to write download file {path}: {source}")]
    WriteDownload {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("artifact SHA-256 mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[cfg(feature = "installer")]
    #[error("failed to create installation staging directory {path}: {source}")]
    CreateInstallStaging {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("failed to open ZIP artifact {path}: {source}")]
    OpenZip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[cfg(feature = "installer")]
    #[error("failed to read ZIP entry {index}: {source}")]
    ReadZipEntry {
        index: usize,
        #[source]
        source: zip::result::ZipError,
    },

    #[cfg(feature = "installer")]
    #[error("failed to read TAR/XZ archive {path}: {source}")]
    ReadTarArchive {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("unsafe archive entry rejected: {entry}")]
    UnsafeArchiveEntry { entry: String },

    #[cfg(feature = "installer")]
    #[error("duplicate or case-colliding archive entry rejected: {entry}")]
    DuplicateArchiveEntry { entry: String },

    #[cfg(feature = "installer")]
    #[error("archive contains {actual} entries, exceeding limit {limit}")]
    TooManyArchiveEntries { actual: usize, limit: usize },

    #[cfg(feature = "installer")]
    #[error("archive expanded size exceeds limit {limit} bytes")]
    ArchiveTooLarge { limit: u64 },

    #[cfg(feature = "installer")]
    #[error("failed to extract archive entry {entry} to {path}: {source}")]
    ExtractArchiveEntry {
        entry: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("required runtime path is missing after extraction: {path}")]
    RequiredPathMissing { path: PathBuf },

    #[cfg(feature = "installer")]
    #[error("failed to serialize installation receipt: {source}")]
    SerializeInstallReceipt {
        #[source]
        source: toml::ser::Error,
    },

    #[cfg(feature = "installer")]
    #[error("failed to write installation receipt {path}: {source}")]
    WriteInstallReceipt {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("refusing to replace existing runtime installation: {path}")]
    InstallAlreadyExists { path: PathBuf },

    #[cfg(feature = "installer")]
    #[error("failed to create final installation parent {path}: {source}")]
    CreateInstallParent {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("failed to atomically commit installation from {staging} to {destination}: {source}")]
    CommitInstall {
        staging: PathBuf,
        destination: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
