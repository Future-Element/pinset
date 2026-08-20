#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProvider {
    pub tool: &'static str,
    pub commands: &'static [&'static str],
    /// Other selected Providers whose command directories must be available to this tool.
    /// Dependencies are declarative and never execute setup hooks or arbitrary code.
    pub dependencies: &'static [&'static str],
    pub capabilities: RuntimeProviderCapabilities,
}

/// The complete set of behaviors implemented by one built-in runtime Provider.
///
/// Keeping these declarations together prevents command routing, discovery, metadata
/// resolution, installation, environment setup, and lock auditing from maintaining
/// separate Provider lists that can drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProviderCapabilities {
    pub command_layout: RuntimeCommandLayout,
    pub metadata: RuntimeMetadataKind,
    pub installer: RuntimeInstallKind,
    pub environment: RuntimeEnvironmentKind,
    pub discovery: &'static [RuntimeDiscoveryRule],
    pub lock_audit: RuntimeLockAuditKind,
    pub provenance: RuntimeProvenanceCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProvenanceCapabilities {
    pub methods: &'static [crate::VerificationMethod],
    /// Whether the consumed upstream metadata supplies a release timestamp that can enforce
    /// `minimum-release-age` without guessing from a version or local file time.
    pub release_time: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDiscoveryRule {
    pub source: &'static str,
    pub kind: RuntimeDiscoveryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeDiscoveryKind {
    SimpleFile { filename: &'static str },
    PythonVersion,
    Fvm,
    Sdkman,
    RustToolchain,
    DotnetGlobalJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCommandLayout {
    NodeNative,
    Python,
    Java,
    Root,
    Bin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMetadataKind {
    Node,
    Npm,
    Go,
    Flutter,
    Java,
    Python,
    Rust,
    Dotnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeInstallKind {
    Node,
    Npm,
    Go,
    Flutter,
    Java,
    Python,
    Rust,
    Dotnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEnvironmentKind {
    None,
    Go,
    Flutter,
    Java,
    Python,
    Dotnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLockAuditKind {
    /// Audit the locked platform artifact, relevant cache entries, install receipt, and
    /// receipt-backed ownership without contacting the Provider.
    ArtifactReceipt,
    /// Reserved for a future constrained Provider that cannot satisfy the shared audit contract.
    Unsupported,
}

const NODE_COMMANDS: &[&str] = &["node", "npm", "npx", "corepack"];
const NODE_DISCOVERY: &[RuntimeDiscoveryRule] = &[
    RuntimeDiscoveryRule {
        source: "nvmrc",
        kind: RuntimeDiscoveryKind::SimpleFile { filename: ".nvmrc" },
    },
    RuntimeDiscoveryRule {
        source: "node-version",
        kind: RuntimeDiscoveryKind::SimpleFile {
            filename: ".node-version",
        },
    },
];
const BUN_DISCOVERY: &[RuntimeDiscoveryRule] = &[RuntimeDiscoveryRule {
    source: "bun-version",
    kind: RuntimeDiscoveryKind::SimpleFile {
        filename: ".bun-version",
    },
}];
const GO_DISCOVERY: &[RuntimeDiscoveryRule] = &[RuntimeDiscoveryRule {
    source: "go-version",
    kind: RuntimeDiscoveryKind::SimpleFile {
        filename: ".go-version",
    },
}];
const FLUTTER_DISCOVERY: &[RuntimeDiscoveryRule] = &[RuntimeDiscoveryRule {
    source: "fvm",
    kind: RuntimeDiscoveryKind::Fvm,
}];
const PYTHON_DISCOVERY: &[RuntimeDiscoveryRule] = &[RuntimeDiscoveryRule {
    source: "python-version",
    kind: RuntimeDiscoveryKind::PythonVersion,
}];
const JAVA_DISCOVERY: &[RuntimeDiscoveryRule] = &[
    RuntimeDiscoveryRule {
        source: "java-version",
        kind: RuntimeDiscoveryKind::SimpleFile {
            filename: ".java-version",
        },
    },
    RuntimeDiscoveryRule {
        source: "sdkmanrc",
        kind: RuntimeDiscoveryKind::Sdkman,
    },
];
const RUST_DISCOVERY: &[RuntimeDiscoveryRule] = &[RuntimeDiscoveryRule {
    source: "rust-toolchain",
    kind: RuntimeDiscoveryKind::RustToolchain,
}];
const DOTNET_DISCOVERY: &[RuntimeDiscoveryRule] = &[RuntimeDiscoveryRule {
    source: "global-json",
    kind: RuntimeDiscoveryKind::DotnetGlobalJson,
}];
const CHECKSUM_METHODS: &[crate::VerificationMethod] = &[crate::VerificationMethod::HttpsChecksum];
const NODE_METHODS: &[crate::VerificationMethod] =
    &[crate::VerificationMethod::OpenPgpSignedChecksum];
const NPM_METHODS: &[crate::VerificationMethod] =
    &[crate::VerificationMethod::NpmRegistrySignature];
const NODE_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "node",
    commands: NODE_COMMANDS,
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::NodeNative,
        metadata: RuntimeMetadataKind::Node,
        installer: RuntimeInstallKind::Node,
        environment: RuntimeEnvironmentKind::None,
        discovery: NODE_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: NODE_METHODS,
            release_time: true,
        },
    },
};
const PNPM_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "pnpm",
    commands: &["pnpm"],
    dependencies: &["node"],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Root,
        metadata: RuntimeMetadataKind::Npm,
        installer: RuntimeInstallKind::Npm,
        environment: RuntimeEnvironmentKind::None,
        discovery: &[],
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: NPM_METHODS,
            release_time: true,
        },
    },
};
const BUN_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "bun",
    commands: &["bun", "bunx"],
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Bin,
        metadata: RuntimeMetadataKind::Npm,
        installer: RuntimeInstallKind::Npm,
        environment: RuntimeEnvironmentKind::None,
        discovery: BUN_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: NPM_METHODS,
            release_time: true,
        },
    },
};
const GO_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "go",
    commands: &["go", "gofmt"],
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Bin,
        metadata: RuntimeMetadataKind::Go,
        installer: RuntimeInstallKind::Go,
        environment: RuntimeEnvironmentKind::Go,
        discovery: GO_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: CHECKSUM_METHODS,
            release_time: false,
        },
    },
};
const FLUTTER_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "flutter",
    commands: &["flutter", "dart"],
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Bin,
        metadata: RuntimeMetadataKind::Flutter,
        installer: RuntimeInstallKind::Flutter,
        environment: RuntimeEnvironmentKind::Flutter,
        discovery: FLUTTER_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: CHECKSUM_METHODS,
            release_time: true,
        },
    },
};
const PYTHON_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "python",
    commands: &["python", "python3", "pip", "pip3"],
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Python,
        metadata: RuntimeMetadataKind::Python,
        installer: RuntimeInstallKind::Python,
        environment: RuntimeEnvironmentKind::Python,
        discovery: PYTHON_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: CHECKSUM_METHODS,
            release_time: true,
        },
    },
};
const JAVA_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "java",
    commands: &[
        "java", "javac", "jar", "javadoc", "javap", "keytool", "jshell",
    ],
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Java,
        metadata: RuntimeMetadataKind::Java,
        installer: RuntimeInstallKind::Java,
        environment: RuntimeEnvironmentKind::Java,
        discovery: JAVA_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: CHECKSUM_METHODS,
            release_time: true,
        },
    },
};
const RUST_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "rust",
    commands: &[
        "rustc",
        "cargo",
        "rustdoc",
        "rustfmt",
        "cargo-fmt",
        "clippy-driver",
        "cargo-clippy",
    ],
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Bin,
        metadata: RuntimeMetadataKind::Rust,
        installer: RuntimeInstallKind::Rust,
        environment: RuntimeEnvironmentKind::None,
        discovery: RUST_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: CHECKSUM_METHODS,
            release_time: true,
        },
    },
};
const DOTNET_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "dotnet",
    commands: &["dotnet"],
    dependencies: &[],
    capabilities: RuntimeProviderCapabilities {
        command_layout: RuntimeCommandLayout::Root,
        metadata: RuntimeMetadataKind::Dotnet,
        installer: RuntimeInstallKind::Dotnet,
        environment: RuntimeEnvironmentKind::Dotnet,
        discovery: DOTNET_DISCOVERY,
        lock_audit: RuntimeLockAuditKind::ArtifactReceipt,
        provenance: RuntimeProvenanceCapabilities {
            methods: CHECKSUM_METHODS,
            release_time: true,
        },
    },
};
const PROVIDERS: &[RuntimeProvider] = &[
    NODE_PROVIDER,
    PNPM_PROVIDER,
    BUN_PROVIDER,
    GO_PROVIDER,
    FLUTTER_PROVIDER,
    PYTHON_PROVIDER,
    JAVA_PROVIDER,
    RUST_PROVIDER,
    DOTNET_PROVIDER,
];

pub fn runtime_providers() -> &'static [RuntimeProvider] {
    PROVIDERS
}

pub fn runtime_provider(tool: &str) -> Option<&'static RuntimeProvider> {
    PROVIDERS.iter().find(|provider| provider.tool == tool)
}

pub fn runtime_provider_for_command(command: &str) -> Option<&'static RuntimeProvider> {
    PROVIDERS
        .iter()
        .find(|provider| provider.commands.contains(&command))
}

/// Return dependencies before dependants, rejecting missing declarations and cycles.
pub fn provider_dependency_order(tool: &str) -> crate::Result<Vec<&'static RuntimeProvider>> {
    let mut visiting = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    let mut ordered = Vec::new();
    visit_provider(tool, &mut visiting, &mut visited, &mut ordered)?;
    Ok(ordered)
}

pub fn validate_provider_selections(
    tools: &std::collections::BTreeMap<String, String>,
) -> crate::Result<()> {
    for tool in tools.keys() {
        let provider =
            runtime_provider(tool).ok_or_else(|| crate::Error::UnsupportedRuntimeProvider {
                provider: tool.clone(),
            })?;
        for dependency in provider_dependency_order(provider.tool)? {
            if dependency.tool != provider.tool && !tools.contains_key(dependency.tool) {
                return Err(crate::Error::ProviderDependencyMissing {
                    tool: provider.tool.to_owned(),
                    dependency: dependency.tool.to_owned(),
                });
            }
        }
    }
    Ok(())
}

pub fn selected_provider_order(
    tools: &std::collections::BTreeMap<String, String>,
) -> crate::Result<Vec<&'static RuntimeProvider>> {
    validate_provider_selections(tools)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut ordered = Vec::new();
    for tool in tools.keys() {
        for provider in provider_dependency_order(tool)? {
            if seen.insert(provider.tool) {
                ordered.push(provider);
            }
        }
    }
    Ok(ordered)
}

fn visit_provider(
    tool: &str,
    visiting: &mut Vec<String>,
    visited: &mut std::collections::BTreeSet<String>,
    ordered: &mut Vec<&'static RuntimeProvider>,
) -> crate::Result<()> {
    if visited.contains(tool) {
        return Ok(());
    }
    if let Some(position) = visiting.iter().position(|candidate| candidate == tool) {
        let mut cycle = visiting[position..].to_vec();
        cycle.push(tool.to_owned());
        return Err(crate::Error::ProviderDependencyCycle {
            cycle: cycle.join(" -> "),
        });
    }
    let provider =
        runtime_provider(tool).ok_or_else(|| crate::Error::UnsupportedRuntimeProvider {
            provider: tool.to_owned(),
        })?;
    visiting.push(tool.to_owned());
    for dependency in provider.dependencies {
        if runtime_provider(dependency).is_none() {
            return Err(crate::Error::ProviderDependencyUnknown {
                tool: tool.to_owned(),
                dependency: (*dependency).to_owned(),
            });
        }
        visit_provider(dependency, visiting, visited, ordered)?;
    }
    visiting.pop();
    visited.insert(tool.to_owned());
    ordered.push(provider);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn node_commands_come_from_one_provider_manifest() {
        let node = runtime_provider("node").expect("node provider");
        assert_eq!(node.commands, ["node", "npm", "npx", "corepack"]);
        for command in node.commands {
            assert_eq!(
                runtime_provider_for_command(command).map(|provider| provider.tool),
                Some("node")
            );
        }
    }

    #[test]
    fn declares_each_supported_provider_command_set() {
        assert_eq!(runtime_provider("pnpm").expect("pnpm").commands, ["pnpm"]);
        assert_eq!(
            runtime_provider("bun").expect("bun").commands,
            ["bun", "bunx"]
        );
        assert_eq!(
            runtime_provider("go").expect("go").commands,
            ["go", "gofmt"]
        );
        assert_eq!(
            runtime_provider_for_command("gofmt").map(|provider| provider.tool),
            Some("go")
        );
        assert_eq!(
            runtime_provider("flutter").expect("flutter").commands,
            ["flutter", "dart"]
        );
        assert_eq!(
            runtime_provider_for_command("dart").map(|provider| provider.tool),
            Some("flutter")
        );
        assert_eq!(
            runtime_provider("python").map(|provider| provider.commands),
            Some(&["python", "python3", "pip", "pip3"][..])
        );
        assert_eq!(
            runtime_provider_for_command("python3").map(|provider| provider.tool),
            Some("python")
        );
        assert_eq!(
            runtime_provider_for_command("pip").map(|provider| provider.tool),
            Some("python")
        );
        assert_eq!(
            runtime_provider_for_command("pip3").map(|provider| provider.tool),
            Some("python")
        );
        assert_eq!(
            runtime_provider("java").map(|provider| provider.commands),
            Some(
                &[
                    "java", "javac", "jar", "javadoc", "javap", "keytool", "jshell"
                ][..]
            )
        );
        assert_eq!(
            runtime_provider_for_command("javac").map(|provider| provider.tool),
            Some("java")
        );
        assert_eq!(
            runtime_provider("rust").map(|provider| provider.commands),
            Some(
                &[
                    "rustc",
                    "cargo",
                    "rustdoc",
                    "rustfmt",
                    "cargo-fmt",
                    "clippy-driver",
                    "cargo-clippy",
                ][..]
            )
        );
        assert_eq!(
            runtime_provider_for_command("cargo-clippy").map(|provider| provider.tool),
            Some("rust")
        );
        assert_eq!(
            runtime_provider("dotnet").map(|provider| provider.commands),
            Some(&["dotnet"][..])
        );
        assert_eq!(
            runtime_provider_for_command("dotnet").map(|provider| provider.tool),
            Some("dotnet")
        );
    }

    #[test]
    fn provider_dependencies_are_acyclic_and_topologically_ordered() {
        let order = provider_dependency_order("pnpm")
            .expect("pnpm dependency graph")
            .into_iter()
            .map(|provider| provider.tool)
            .collect::<Vec<_>>();
        assert_eq!(order, ["node", "pnpm"]);

        let selected = std::collections::BTreeMap::from([
            ("node".to_owned(), "24".to_owned()),
            ("pnpm".to_owned(), "11".to_owned()),
        ]);
        assert!(validate_provider_selections(&selected).is_ok());
        let missing = std::collections::BTreeMap::from([("pnpm".to_owned(), "11".to_owned())]);
        assert!(matches!(
            validate_provider_selections(&missing),
            Err(crate::Error::ProviderDependencyMissing { .. })
        ));
    }

    #[test]
    fn provider_tools_and_commands_are_globally_unique() {
        let mut tools = HashSet::new();
        let mut commands = HashSet::new();
        for provider in runtime_providers() {
            assert!(
                tools.insert(provider.tool),
                "duplicate tool {}",
                provider.tool
            );
            assert!(
                !provider.commands.is_empty(),
                "empty provider {}",
                provider.tool
            );
            for command in provider.commands {
                assert!(
                    commands.insert(*command),
                    "command {command} is declared by multiple providers"
                );
            }
        }
    }

    #[test]
    fn provider_specific_discovery_is_declared_in_provider_manifests() {
        assert_eq!(
            runtime_provider("node")
                .expect("node")
                .capabilities
                .discovery,
            NODE_DISCOVERY
        );
        assert_eq!(
            runtime_provider("python")
                .expect("python")
                .capabilities
                .discovery,
            PYTHON_DISCOVERY
        );
        assert_eq!(
            runtime_provider("dotnet")
                .expect("dotnet")
                .capabilities
                .discovery,
            DOTNET_DISCOVERY
        );
    }

    #[test]
    fn every_provider_declares_the_complete_v18_capability_model() {
        assert_eq!(runtime_providers().len(), 9);
        for provider in runtime_providers() {
            assert_eq!(
                provider.capabilities.lock_audit,
                RuntimeLockAuditKind::ArtifactReceipt,
                "{} must participate in the shared lock audit contract",
                provider.tool
            );
            assert!(
                !provider.capabilities.provenance.methods.is_empty(),
                "{} must declare its verification method",
                provider.tool
            );
        }
        assert!(
            !runtime_provider("go")
                .expect("Go")
                .capabilities
                .provenance
                .release_time
        );
        assert!(
            runtime_provider("node")
                .expect("Node")
                .capabilities
                .provenance
                .release_time
        );
    }
}
