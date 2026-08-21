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

    #[error("unsupported pinset.toml schema {actual}; this version supports schemas 1, 2, 3 and 4")]
    UnsupportedSchema { actual: u32 },

    #[error("invalid pinset.toml configuration: {reason}")]
    InvalidProjectConfig { reason: String },

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

    #[error("unsupported global.toml schema {actual}; this version supports schemas 1, 2 and 3")]
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
    #[error("unsupported pinset.lock schema {actual}; this version supports schemas 1, 2 and 3")]
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
        "tool \"{tool}\" is not declared in strict project {config_path}; add it with `pinset use {tool}@<selector>` or explicitly enable project fallback"
    )]
    ProjectToolSelectionRequired { tool: String, config_path: PathBuf },

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
    #[error(
        "unsupported source provider \"{provider}\"; expected one of: node, go, python, flutter"
    )]
    UnsupportedSourceProvider { provider: String },

    #[error("unsupported runtime provider \"{provider}\"")]
    UnsupportedRuntimeProvider { provider: String },

    #[error("provider {tool} requires selected provider {dependency}")]
    ProviderDependencyMissing { tool: String, dependency: String },

    #[error("provider {tool} declares unknown dependency {dependency}")]
    ProviderDependencyUnknown { tool: String, dependency: String },

    #[error("provider dependency graph contains a cycle: {cycle}")]
    ProviderDependencyCycle { cycle: String },

    #[cfg(feature = "provider-registry")]
    #[error("failed to read Provider Registry {path}: {source}")]
    ReadProviderRegistry {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "provider-registry")]
    #[error("Provider Registry signature is invalid: {reason}")]
    ProviderRegistrySignatureInvalid { reason: String },

    #[cfg(feature = "provider-registry")]
    #[error("Provider Registry is invalid: {reason}")]
    ProviderRegistryInvalid { reason: String },

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

    #[cfg(any(
        feature = "sources",
        feature = "rust-metadata",
        feature = "dotnet-metadata"
    ))]
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

    #[cfg(feature = "go-provider")]
    #[error("invalid exact Go version \"{version}\"; expected x.y.z without a leading 'go' or 'v'")]
    InvalidGoVersion { version: String },

    #[cfg(feature = "go-provider")]
    #[error("unsupported Go target \"{target}\"")]
    UnsupportedGoTarget { target: String },

    #[cfg(feature = "go-metadata")]
    #[error(
        "invalid Go selector \"{selector}\"; expected x.y.z, a major/minor prefix, latest or current"
    )]
    InvalidGoSelector { selector: String },

    #[cfg(feature = "go-metadata")]
    #[error("official Go index contains no supported release matching \"{selector}\"")]
    GoSelectorNotFound { selector: String },

    #[cfg(feature = "go-metadata")]
    #[error("failed to request Go download metadata {url}: {source}")]
    GoMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "go-metadata")]
    #[error("failed while reading Go download metadata {url}: {source}")]
    GoMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "go-metadata")]
    #[error("Go download index exceeds {limit} bytes")]
    GoMetadataTooLarge { limit: u64 },

    #[cfg(feature = "go-metadata")]
    #[error("invalid Go download index: {reason}")]
    InvalidGoIndex { reason: String },

    #[cfg(feature = "flutter-provider")]
    #[error("invalid exact Flutter version \"{version}\"; expected x.y.z")]
    InvalidFlutterVersion { version: String },

    #[cfg(feature = "flutter-provider")]
    #[error(
        "unsupported Flutter target \"{target}\"; Flutter upstream does not publish an official SDK for this target"
    )]
    UnsupportedFlutterTarget { target: String },

    #[cfg(feature = "python-provider")]
    #[error("invalid exact Python distribution \"{version}\"; expected x.y.z+YYYYMMDD")]
    InvalidPythonVersion { version: String },

    #[cfg(feature = "python-provider")]
    #[error("unsupported Python target \"{target}\"")]
    UnsupportedPythonTarget { target: String },

    #[cfg(feature = "python-metadata")]
    #[error("invalid Python selector \"{selector}\"")]
    InvalidPythonSelector { selector: String },

    #[cfg(feature = "python-metadata")]
    #[error("Python metadata contains no stable release matching \"{selector}\"")]
    PythonSelectorNotFound { selector: String },

    #[cfg(feature = "python-metadata")]
    #[error("failed to request Python release metadata {url}: {source}")]
    PythonMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "python-metadata")]
    #[error("failed while reading Python release metadata {url}: {source}")]
    PythonMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "python-metadata")]
    #[error("Python release metadata exceeds {limit} bytes")]
    PythonMetadataTooLarge { limit: u64 },

    #[cfg(feature = "python-metadata")]
    #[error("invalid Python release metadata: {reason}")]
    InvalidPythonIndex { reason: String },

    #[cfg(feature = "java-provider")]
    #[error("invalid exact Java version \"{version}\"; expected x.y.z+build")]
    InvalidJavaVersion { version: String },

    #[cfg(feature = "java-provider")]
    #[error("unsupported Java target \"{target}\"")]
    UnsupportedJavaTarget { target: String },

    #[cfg(feature = "java-provider")]
    #[error("invalid Eclipse Temurin artifact identity: {reason}")]
    InvalidJavaArtifact { reason: String },

    #[cfg(feature = "java-metadata")]
    #[error(
        "invalid Java selector \"{selector}\"; expected a feature, feature/minor prefix, update, exact build, lts, latest or current"
    )]
    InvalidJavaSelector { selector: String },

    #[cfg(feature = "java-metadata")]
    #[error("Adoptium metadata contains no supported Temurin JDK matching \"{selector}\"")]
    JavaSelectorNotFound { selector: String },

    #[cfg(feature = "java-metadata")]
    #[error("failed to request Adoptium metadata {url}: {source}")]
    JavaMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "java-metadata")]
    #[error("failed while reading Adoptium metadata {url}: {source}")]
    JavaMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "java-metadata")]
    #[error("Adoptium metadata exceeds {limit} bytes")]
    JavaMetadataTooLarge { limit: u64 },

    #[cfg(feature = "java-metadata")]
    #[error("invalid Adoptium metadata: {reason}")]
    InvalidJavaIndex { reason: String },

    #[cfg(feature = "rust-provider")]
    #[error("invalid exact Rust version \"{version}\"; expected x.y.z")]
    InvalidRustVersion { version: String },

    #[cfg(feature = "rust-provider")]
    #[error("unsupported Rust target \"{target}\"")]
    UnsupportedRustTarget { target: String },

    #[cfg(feature = "rust-provider")]
    #[error("invalid official Rust artifact identity: {reason}")]
    InvalidRustArtifact { reason: String },

    #[cfg(feature = "rust-metadata")]
    #[error(
        "invalid Rust selector \"{selector}\"; expected x.y.z, a major/minor prefix, stable, latest or current"
    )]
    InvalidRustSelector { selector: String },

    #[cfg(feature = "rust-metadata")]
    #[error("official Rust manifests contain no stable release matching \"{selector}\"")]
    RustSelectorNotFound { selector: String },

    #[cfg(feature = "rust-metadata")]
    #[error("failed to request official Rust metadata {url}: {source}")]
    RustMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "rust-metadata")]
    #[error("failed while reading official Rust metadata {url}: {source}")]
    RustMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "rust-metadata")]
    #[error("official Rust metadata exceeds {limit} bytes")]
    RustMetadataTooLarge { limit: u64 },

    #[cfg(feature = "rust-metadata")]
    #[error("invalid official Rust metadata: {reason}")]
    InvalidRustIndex { reason: String },

    #[cfg(feature = "dotnet-provider")]
    #[error("invalid exact .NET SDK version \"{version}\"; expected x.y.zzz")]
    InvalidDotnetVersion { version: String },

    #[cfg(feature = "dotnet-provider")]
    #[error("unsupported .NET SDK target \"{target}\"")]
    UnsupportedDotnetTarget { target: String },

    #[cfg(feature = "dotnet-provider")]
    #[error("invalid official .NET SDK artifact identity: {reason}")]
    InvalidDotnetArtifact { reason: String },

    #[cfg(feature = "dotnet-metadata")]
    #[error(
        "invalid .NET SDK selector \"{selector}\"; expected x.y.zzz, a major/channel prefix, lts, latest or current"
    )]
    InvalidDotnetSelector { selector: String },

    #[cfg(feature = "dotnet-metadata")]
    #[error("official .NET metadata contains no supported SDK matching \"{selector}\"")]
    DotnetSelectorNotFound { selector: String },

    #[cfg(feature = "dotnet-metadata")]
    #[error("failed to request official .NET metadata {url}: {source}")]
    DotnetMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "dotnet-metadata")]
    #[error("failed while reading official .NET metadata {url}: {source}")]
    DotnetMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "dotnet-metadata")]
    #[error("official .NET metadata exceeds {limit} bytes")]
    DotnetMetadataTooLarge { limit: u64 },

    #[cfg(feature = "dotnet-metadata")]
    #[error("invalid official .NET metadata: {reason}")]
    InvalidDotnetIndex { reason: String },

    #[error("project Python environment {path} is not owned by Pinset")]
    PythonEnvironmentNotOwned { path: PathBuf },

    #[error(
        "project Python environment {path} selects {actual}, but the project locks {expected}; run `pinset venv recreate`"
    )]
    PythonEnvironmentMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("project Python environment {path} is missing; run `pinset venv create`")]
    PythonEnvironmentMissing { path: PathBuf },

    #[error("project Python environment requires a project-level Python selection in {path}")]
    PythonEnvironmentSelectionMissing { path: PathBuf },

    #[error("failed to run the managed Python interpreter while creating {path}: {source}")]
    PythonEnvironmentCreate {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("managed Python failed to create {path} (exit code {code})")]
    PythonEnvironmentCreateFailed { path: PathBuf, code: i32 },

    #[error("failed to remove managed Python environment {path}: {source}")]
    RemovePythonEnvironment {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read Python environment marker {path}: {source}")]
    ReadPythonEnvironmentMarker {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid Python environment marker {path}: {reason}")]
    InvalidPythonEnvironmentMarker { path: PathBuf, reason: String },

    #[error("failed to write Python environment marker {path}: {source}")]
    WritePythonEnvironmentMarker {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "flutter-metadata")]
    #[error(
        "invalid Flutter selector \"{selector}\"; expected x.y.z, a major/minor prefix, latest or current"
    )]
    InvalidFlutterSelector { selector: String },

    #[cfg(feature = "flutter-metadata")]
    #[error("official Flutter indexes contain no stable release matching \"{selector}\"")]
    FlutterSelectorNotFound { selector: String },

    #[cfg(feature = "flutter-metadata")]
    #[error("failed to request Flutter release metadata {url}: {source}")]
    FlutterMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "flutter-metadata")]
    #[error("failed while reading Flutter release metadata {url}: {source}")]
    FlutterMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "flutter-metadata")]
    #[error("Flutter release index exceeds {limit} bytes")]
    FlutterMetadataTooLarge { limit: u64 },

    #[cfg(feature = "flutter-metadata")]
    #[error("invalid Flutter release index: {reason}")]
    InvalidFlutterIndex { reason: String },

    #[error(
        "refusing to run `{command}` against a Pinset-managed Flutter SDK; select another version with `pinset use flutter@<version>` or `pinset global flutter@<version>`"
    )]
    ManagedFlutterMutation { command: String },

    #[cfg(feature = "node-metadata")]
    #[error(
        "invalid Node.js selector \"{selector}\"; expected x.y.z, a major/minor prefix, lts or current"
    )]
    InvalidNodeSelector { selector: String },

    #[cfg(feature = "node-metadata")]
    #[error("official Node.js index contains no supported release matching \"{selector}\"")]
    NodeSelectorNotFound { selector: String },

    #[cfg(feature = "node-provider")]
    #[error(
        "unsupported Node.js target \"{target}\"; expected windows/linux/macos with x86_64/aarch64"
    )]
    UnsupportedNodeTarget { target: String },

    #[cfg(feature = "node-provider")]
    #[error("failed to read Node.js installation directory {path}: {source}")]
    ReadNodeInstallDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "node-provider")]
    #[error("Node.js {version} is not installed by Pinset")]
    NodeVersionNotInstalled { version: String },

    #[cfg(feature = "node-provider")]
    #[error("refusing to uninstall Node.js {version}; it is selected by {references}")]
    NodeVersionInUse { version: String, references: String },

    #[cfg(feature = "node-provider")]
    #[error("unsafe or unowned Node.js installation entry: {path}")]
    UnsafeNodeInstallEntry { path: PathBuf },

    #[cfg(feature = "node-provider")]
    #[error("failed to remove Node.js installation {path}: {source}")]
    RemoveNodeInstall {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read {tool} installation directory {path}: {source}")]
    ReadToolInstallDirectory {
        tool: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{tool} {version} is not installed by Pinset")]
    ToolVersionNotInstalled { tool: String, version: String },

    #[error("invalid {tool} version {version:?}")]
    InvalidToolVersion { tool: String, version: String },

    #[error("refusing to uninstall {tool} {version}; it is selected by {references}")]
    ToolVersionInUse {
        tool: String,
        version: String,
        references: String,
    },

    #[error("unsafe or unowned {tool} installation entry: {path}")]
    UnsafeToolInstallEntry { tool: String, path: PathBuf },

    #[error("failed to remove {tool} installation {path}: {source}")]
    RemoveToolInstall {
        tool: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

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
    #[error("official Node.js release index exceeds {limit} bytes")]
    NodeIndexTooLarge { limit: u64 },

    #[cfg(feature = "node-metadata")]
    #[error("invalid official Node.js release index: {reason}")]
    InvalidNodeIndex { reason: String },

    #[cfg(feature = "node-metadata")]
    #[error("invalid official Node.js SHASUMS: {reason}")]
    InvalidNodeShasums { reason: String },

    #[cfg(feature = "node-metadata")]
    #[error("Node.js release signature is invalid: {reason}")]
    NodeSignatureInvalid { reason: String },

    #[cfg(feature = "node-metadata")]
    #[error("Node.js release signature uses an untrusted signer: {signer}")]
    NodeSignerUntrusted { signer: String },

    #[cfg(feature = "node-metadata")]
    #[error("embedded Node.js release trust store is invalid: {reason}")]
    NodeTrustStoreInvalid { reason: String },

    #[cfg(feature = "node-metadata")]
    #[error("Node.js {version} SHASUMS does not contain {filename}")]
    NodeChecksumMissing { version: String, filename: String },

    #[cfg(feature = "npm-metadata")]
    #[error(
        "invalid {tool} selector {selector:?}; expected an exact, major, minor, latest or current selector"
    )]
    InvalidNpmToolSelector { tool: String, selector: String },

    #[cfg(feature = "npm-metadata")]
    #[error("no supported {tool} release matches selector {selector:?}")]
    NpmToolSelectorNotFound { tool: String, selector: String },

    #[cfg(feature = "npm-metadata")]
    #[error("failed to request npm registry metadata {url}: {source}")]
    NpmMetadataRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[cfg(feature = "npm-metadata")]
    #[error("failed while reading npm registry metadata {url}: {source}")]
    NpmMetadataRead {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "npm-metadata")]
    #[error("npm registry metadata exceeds {limit} bytes")]
    NpmMetadataTooLarge { limit: u64 },

    #[cfg(feature = "npm-metadata")]
    #[error("invalid npm registry metadata for {package}: {reason}")]
    InvalidNpmMetadata { package: String, reason: String },

    #[cfg(feature = "npm-metadata")]
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

    #[cfg(any(feature = "installer", feature = "lockfile", feature = "npm-metadata"))]
    #[error(
        "invalid artifact integrity \"{value}\"; expected sha256:<hex>, sha512:<hex> or npm sha512-<base64>"
    )]
    InvalidArtifactIntegrity { value: String },

    #[cfg(any(
        feature = "installer",
        feature = "go-metadata",
        feature = "node-metadata",
        feature = "npm-metadata",
        feature = "rust-metadata",
        feature = "dotnet-metadata"
    ))]
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
    #[error("failed to read download cache {path}: {source}")]
    ReadDownloadCache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("unsafe download cache entry rejected: {path}")]
    UnsafeDownloadCacheEntry { path: PathBuf },

    #[cfg(feature = "installer")]
    #[error("failed to remove download cache entry {path}: {source}")]
    RemoveDownloadCacheEntry {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("{entries} corrupt cache archive(s) found; run `pinset cache repair`")]
    DownloadCacheCorrupt { entries: usize },

    #[cfg(feature = "installer")]
    #[error("artifact integrity mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[cfg(feature = "installer")]
    #[error("failed to create installation staging directory {path}: {source}")]
    CreateInstallStaging {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("failed to open installation lock {path}: {source}")]
    OpenInstallLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "installer")]
    #[error("failed to acquire installation lock {path}: {source}")]
    AcquireInstallLock {
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

    #[cfg(feature = "installer")]
    #[error("failed to create runtime command alias {destination} from {source_path}: {source}")]
    CreateRuntimeAlias {
        source_path: PathBuf,
        destination: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
