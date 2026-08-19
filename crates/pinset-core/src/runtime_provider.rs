#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProvider {
    pub tool: &'static str,
    pub commands: &'static [&'static str],
    pub command_layout: RuntimeCommandLayout,
    pub metadata: RuntimeMetadataKind,
    pub installer: RuntimeInstallKind,
    pub environment: RuntimeEnvironmentKind,
    pub discovery: &'static [RuntimeDiscoveryRule],
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
const NODE_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "node",
    commands: NODE_COMMANDS,
    command_layout: RuntimeCommandLayout::NodeNative,
    metadata: RuntimeMetadataKind::Node,
    installer: RuntimeInstallKind::Node,
    environment: RuntimeEnvironmentKind::None,
    discovery: NODE_DISCOVERY,
};
const PNPM_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "pnpm",
    commands: &["pnpm"],
    command_layout: RuntimeCommandLayout::Root,
    metadata: RuntimeMetadataKind::Npm,
    installer: RuntimeInstallKind::Npm,
    environment: RuntimeEnvironmentKind::None,
    discovery: &[],
};
const BUN_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "bun",
    commands: &["bun", "bunx"],
    command_layout: RuntimeCommandLayout::Bin,
    metadata: RuntimeMetadataKind::Npm,
    installer: RuntimeInstallKind::Npm,
    environment: RuntimeEnvironmentKind::None,
    discovery: BUN_DISCOVERY,
};
const GO_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "go",
    commands: &["go", "gofmt"],
    command_layout: RuntimeCommandLayout::Bin,
    metadata: RuntimeMetadataKind::Go,
    installer: RuntimeInstallKind::Go,
    environment: RuntimeEnvironmentKind::Go,
    discovery: GO_DISCOVERY,
};
const FLUTTER_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "flutter",
    commands: &["flutter", "dart"],
    command_layout: RuntimeCommandLayout::Bin,
    metadata: RuntimeMetadataKind::Flutter,
    installer: RuntimeInstallKind::Flutter,
    environment: RuntimeEnvironmentKind::Flutter,
    discovery: FLUTTER_DISCOVERY,
};
const PYTHON_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "python",
    commands: &["python", "python3", "pip", "pip3"],
    command_layout: RuntimeCommandLayout::Python,
    metadata: RuntimeMetadataKind::Python,
    installer: RuntimeInstallKind::Python,
    environment: RuntimeEnvironmentKind::Python,
    discovery: PYTHON_DISCOVERY,
};
const JAVA_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "java",
    commands: &[
        "java", "javac", "jar", "javadoc", "javap", "keytool", "jshell",
    ],
    command_layout: RuntimeCommandLayout::Java,
    metadata: RuntimeMetadataKind::Java,
    installer: RuntimeInstallKind::Java,
    environment: RuntimeEnvironmentKind::Java,
    discovery: JAVA_DISCOVERY,
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
    command_layout: RuntimeCommandLayout::Bin,
    metadata: RuntimeMetadataKind::Rust,
    installer: RuntimeInstallKind::Rust,
    environment: RuntimeEnvironmentKind::None,
    discovery: RUST_DISCOVERY,
};
const DOTNET_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "dotnet",
    commands: &["dotnet"],
    command_layout: RuntimeCommandLayout::Root,
    metadata: RuntimeMetadataKind::Dotnet,
    installer: RuntimeInstallKind::Dotnet,
    environment: RuntimeEnvironmentKind::Dotnet,
    discovery: DOTNET_DISCOVERY,
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
            runtime_provider("node").expect("node").discovery,
            NODE_DISCOVERY
        );
        assert_eq!(
            runtime_provider("python").expect("python").discovery,
            PYTHON_DISCOVERY
        );
        assert_eq!(
            runtime_provider("dotnet").expect("dotnet").discovery,
            DOTNET_DISCOVERY
        );
    }
}
