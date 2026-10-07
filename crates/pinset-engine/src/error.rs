use std::path::PathBuf;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid .pinset/config.toml configuration: {reason}")]
    InvalidProjectConfig { reason: String },

    #[error("invalid .pinset/lock.toml: {reason}")]
    InvalidLockfile { reason: String },

    #[error(
        "{tool}@{version} exists, but its official distribution has no installable artifact for {target}"
    )]
    LockedArtifactMissing {
        tool: String,
        version: String,
        target: String,
    },

    #[error(
        "unsupported source provider \"{provider}\"; expected one of: node, go, python, flutter"
    )]
    UnsupportedSourceProvider { provider: String },

    #[error("invalid base URL \"{url}\": {reason}")]
    InvalidSourceBaseUrl { url: String, reason: String },

    #[error("invalid exact Node.js version \"{version}\"; expected x.y.z without a leading 'v'")]
    InvalidNodeVersion { version: String },

    #[error("invalid exact Go version \"{version}\"; expected x.y.z without a leading 'go' or 'v'")]
    InvalidGoVersion { version: String },

    #[error("unsupported Go target \"{target}\"")]
    UnsupportedGoTarget { target: String },

    #[error(
        "Go {version} exists in the official archive, but its index publishes no verifiable SHA-256 artifact for a Pinset target"
    )]
    GoArtifactsUnverifiable { version: String },

    #[error("invalid Go selector \"{selector}\"; expected x.y.z, a major/minor prefix, latest")]
    InvalidGoSelector { selector: String },

    #[error("official Go index contains no supported release matching \"{selector}\"")]
    GoSelectorNotFound { selector: String },

    #[error("failed to request Go download metadata {url}: {source}")]
    GoMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading Go download metadata {url}: {source}")]
    GoMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Go download index exceeds {limit} bytes")]
    GoMetadataTooLarge { limit: u64 },

    #[error("invalid Go download index: {reason}")]
    InvalidGoIndex { reason: String },

    #[error("invalid exact Flutter version \"{version}\"; expected x.y.z")]
    InvalidFlutterVersion { version: String },

    #[error(
        "unsupported Flutter target \"{target}\"; Flutter upstream does not publish an official SDK for this target"
    )]
    UnsupportedFlutterTarget { target: String },

    #[error("invalid exact Python distribution \"{version}\"; expected x.y.z or x.y.z+YYYYMMDD")]
    InvalidPythonVersion { version: String },

    #[error("unsupported Python target \"{target}\"")]
    UnsupportedPythonTarget { target: String },

    #[error(
        "Python {version} exists in the official CPython archive, but no supported artifact is available for {distribution}"
    )]
    PythonDistributionUnavailable {
        version: String,
        distribution: String,
    },

    #[error("invalid Python selector \"{selector}\"")]
    InvalidPythonSelector { selector: String },

    #[error("Python metadata contains no stable release matching \"{selector}\"")]
    PythonSelectorNotFound { selector: String },

    #[error("failed to request Python release metadata {url}: {source}")]
    PythonMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading Python release metadata {url}: {source}")]
    PythonMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Python release metadata exceeds {limit} bytes")]
    PythonMetadataTooLarge { limit: u64 },

    #[error("invalid Python release metadata: {reason}")]
    InvalidPythonIndex { reason: String },

    #[error("invalid exact Java version \"{version}\"; expected x.y.z+build")]
    InvalidJavaVersion { version: String },

    #[error("unsupported Java target \"{target}\"")]
    UnsupportedJavaTarget { target: String },

    #[error("invalid Eclipse Temurin artifact identity: {reason}")]
    InvalidJavaArtifact { reason: String },

    #[error(
        "invalid Java selector \"{selector}\"; expected a feature, feature/minor prefix, update, exact build, lts, latest"
    )]
    InvalidJavaSelector { selector: String },

    #[error("Adoptium metadata contains no supported Temurin JDK matching \"{selector}\"")]
    JavaSelectorNotFound { selector: String },

    #[error("failed to request Adoptium metadata {url}: {source}")]
    JavaMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading Adoptium metadata {url}: {source}")]
    JavaMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Adoptium metadata exceeds {limit} bytes")]
    JavaMetadataTooLarge { limit: u64 },

    #[error("invalid Adoptium metadata: {reason}")]
    InvalidJavaIndex { reason: String },

    #[error("invalid exact Rust version \"{version}\"; expected x.y.z")]
    InvalidRustVersion { version: String },

    #[error("unsupported Rust target \"{target}\"")]
    UnsupportedRustTarget { target: String },

    #[error("invalid official Rust artifact identity: {reason}")]
    InvalidRustArtifact { reason: String },

    #[error(
        "invalid Rust selector \"{selector}\"; expected x.y.z, a major/minor prefix, stable, latest"
    )]
    InvalidRustSelector { selector: String },

    #[error("official Rust manifests contain no stable release matching \"{selector}\"")]
    RustSelectorNotFound { selector: String },

    #[error("failed to request official Rust metadata {url}: {source}")]
    RustMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading official Rust metadata {url}: {source}")]
    RustMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("official Rust metadata exceeds {limit} bytes")]
    RustMetadataTooLarge { limit: u64 },

    #[error("invalid official Rust metadata: {reason}")]
    InvalidRustIndex { reason: String },

    #[error(
        "invalid Flutter selector \"{selector}\"; expected x.y.z, a major/minor prefix, latest"
    )]
    InvalidFlutterSelector { selector: String },

    #[error("official Flutter indexes contain no stable release matching \"{selector}\"")]
    FlutterSelectorNotFound { selector: String },

    #[error("failed to request Flutter release metadata {url}: {source}")]
    FlutterMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading Flutter release metadata {url}: {source}")]
    FlutterMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Flutter release index exceeds {limit} bytes")]
    FlutterMetadataTooLarge { limit: u64 },

    #[error("invalid Flutter release index: {reason}")]
    InvalidFlutterIndex { reason: String },

    #[error(
        "invalid Node.js selector \"{selector}\"; expected x.y.z, a major/minor prefix, lts or latest"
    )]
    InvalidNodeSelector { selector: String },

    #[error("official Node.js index contains no supported release matching \"{selector}\"")]
    NodeSelectorNotFound { selector: String },

    #[error(
        "unsupported Node.js target \"{target}\"; expected windows/linux/macos with x86_64/aarch64"
    )]
    UnsupportedNodeTarget { target: String },

    #[error("failed to request official Node.js metadata {url}: {source}")]
    NodeMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading official Node.js metadata {url}: {source}")]
    NodeMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("official Node.js SHASUMS exceeds {limit} bytes")]
    NodeMetadataTooLarge { limit: u64 },

    #[error("official Node.js release index exceeds {limit} bytes")]
    NodeIndexTooLarge { limit: u64 },

    #[error("invalid official Node.js release index: {reason}")]
    InvalidNodeIndex { reason: String },

    #[error("invalid official Node.js SHASUMS: {reason}")]
    InvalidNodeShasums { reason: String },

    #[error("Node.js release signature is invalid: {reason}")]
    NodeSignatureInvalid { reason: String },

    #[error("Node.js release signature uses an untrusted signer: {signer}")]
    NodeSignerUntrusted { signer: String },

    #[error("embedded Node.js release trust store is invalid: {reason}")]
    NodeTrustStoreInvalid { reason: String },

    #[error("Node.js {version} SHASUMS does not contain {filename}")]
    NodeChecksumMissing { version: String, filename: String },

    #[error(
        "invalid {tool} selector {selector:?}; expected an exact, major, minor, latest selector"
    )]
    InvalidNpmToolSelector { tool: String, selector: String },

    #[error("no supported {tool} release matches selector {selector:?}")]
    NpmToolSelectorNotFound { tool: String, selector: String },

    #[error("failed to request npm registry metadata {url}: {source}")]
    NpmMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading npm registry metadata {url}: {source}")]
    NpmMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("npm registry metadata exceeds {limit} bytes")]
    NpmMetadataTooLarge { limit: u64 },

    #[error("invalid npm registry metadata for {package}: {reason}")]
    InvalidNpmMetadata { package: String, reason: String },

    #[error("npm registry signature verification failed for {package}@{version}: {reason}")]
    NpmSignatureVerification {
        package: String,
        version: String,
        reason: String,
    },

    #[error(
        "verification policy for {tool} requires {required}, but the lock provides only {actual}"
    )]
    VerificationPolicyViolation {
        tool: String,
        required: String,
        actual: String,
    },

    #[error(
        "refusing to downgrade {tool} verification from {previous} to {next}; keep the stronger evidence or create a fresh lock explicitly"
    )]
    VerificationDowngrade {
        tool: String,
        previous: String,
        next: String,
    },

    #[error(
        "minimum release age cannot be enforced for {tool}: the Provider supplied no release timestamp"
    )]
    ReleaseAgeUnavailable { tool: String },

    #[error(
        "{tool} release {released_at} is newer than the required minimum release age {required}"
    )]
    ReleaseTooNew {
        tool: String,
        released_at: String,
        required: String,
    },

    #[error("invalid install path segment for {field}: \"{value}\"")]
    InvalidInstallSegment { field: &'static str, value: String },

    #[error("invalid archive strip-components value {value}; maximum is 8")]
    InvalidStripComponents { value: usize },

    #[error("an install request must declare at least one required runtime path")]
    RequiredPathsEmpty,

    #[error("an artifact must declare at least one download source")]
    ArtifactSourcesEmpty,

    #[error("invalid artifact source id \"{value}\"")]
    InvalidArtifactSourceId { value: String },

    #[error("duplicate artifact source id \"{value}\"")]
    DuplicateArtifactSourceId { value: String },

    #[error("all artifact sources failed ({attempted}); last error: {last_error}")]
    ArtifactSourcesExhausted {
        attempted: String,
        last_error: String,
    },

    #[error("offline mode requires cached artifact {integrity}")]
    OfflineArtifactMissing { integrity: String },

    #[error("required runtime path must be relative and contained: {path}")]
    InvalidRequiredPath { path: PathBuf },

    #[error("invalid SHA-256 value \"{value}\"; expected 64 hexadecimal characters")]
    InvalidSha256 { value: String },

    #[error("failed to build the HTTP client: {source}")]
    HttpClient {
        #[source]
        source: reqwest::Error,
    },

    #[error("invalid network configuration: {reason}")]
    InvalidNetworkConfig { reason: String },

    #[error("failed to request artifact {url}: {source}")]
    DownloadRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed while reading artifact {url}: {source}")]
    DownloadRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "artifact download failed after {attempts} attempts; {downloaded_bytes} partial bytes retained for the next run: {source}"
    )]
    DownloadRetriesExhausted {
        attempts: usize,
        downloaded_bytes: u64,
        #[source]
        source: Box<Error>,
    },

    #[error("artifact from {url} exceeds download limit {limit} bytes")]
    DownloadTooLarge { url: String, limit: u64 },

    #[error("failed to write download file {path}: {source}")]
    WriteDownload {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read download cache {path}: {source}")]
    ReadDownloadCache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unsafe download cache entry rejected: {path}")]
    UnsafeDownloadCacheEntry { path: PathBuf },

    #[error("failed to remove download cache entry {path}: {source}")]
    RemoveDownloadCacheEntry {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("artifact integrity mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("failed to create installation staging directory {path}: {source}")]
    CreateInstallStaging {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to open installation lock {path}: {source}")]
    OpenInstallLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to acquire installation lock {path}: {source}")]
    AcquireInstallLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to open ZIP artifact {path}: {source}")]
    OpenZip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("failed to read ZIP entry {index}: {source}")]
    ReadZipEntry {
        index: usize,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("failed to read TAR/XZ archive {path}: {source}")]
    ReadTarArchive {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unsafe archive entry rejected: {entry}")]
    UnsafeArchiveEntry { entry: String },

    #[error("duplicate or case-colliding archive entry rejected: {entry}")]
    DuplicateArchiveEntry { entry: String },

    #[error("archive contains {actual} entries, exceeding limit {limit}")]
    TooManyArchiveEntries { actual: usize, limit: usize },

    #[error("archive expanded size exceeds limit {limit} bytes")]
    ArchiveTooLarge { limit: u64 },

    #[error("failed to extract archive entry {entry} to {path}: {source}")]
    ExtractArchiveEntry {
        entry: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid artifact URL: {url}")]
    InvalidArtifactUrl { url: String },

    #[error("failed to start {format} archive extraction for {path}: {source}")]
    NativeArchiveExtract {
        format: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{format} archive extraction failed for {path} with exit code {code}")]
    NativeArchiveExtractFailed {
        format: String,
        path: PathBuf,
        code: i32,
    },

    #[error("required runtime path is missing after extraction: {path}")]
    RequiredPathMissing { path: PathBuf },

    #[error("failed to serialize installation receipt: {source}")]
    SerializeInstallReceipt {
        #[source]
        source: toml::ser::Error,
    },

    #[error("failed to write installation receipt {path}: {source}")]
    WriteInstallReceipt {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("refusing to replace existing runtime installation: {path}")]
    InstallAlreadyExists { path: PathBuf },

    #[error("failed to create final installation parent {path}: {source}")]
    CreateInstallParent {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to atomically commit installation from {staging} to {destination}: {source}")]
    CommitInstall {
        staging: PathBuf,
        destination: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to create runtime command alias {destination} from {source_path}: {source}")]
    CreateRuntimeAlias {
        source_path: PathBuf,
        destination: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Core(#[from] pinset_core::Error),
    #[error(transparent)]
    Environment(#[from] pinset_env::Error),
    #[error("{code}: {message}")]
    Service { code: &'static str, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialize(#[from] toml::ser::Error),
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Service { code, .. } => code,
            Self::Core(error) => error.code(),
            Self::Environment(_) => "PINSET_ENVIRONMENT",
            Self::Io(_) => "PINSET_IO",
            Self::Parse(_) | Self::Serialize(_) | Self::Json(_) => "PINSET_PROTOCOL_INVALID",
            _ => "PINSET_PROVIDER_FAILED",
        }
    }
}
