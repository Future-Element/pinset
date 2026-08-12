#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProvider {
    pub tool: &'static str,
    pub commands: &'static [&'static str],
}

const NODE_COMMANDS: &[&str] = &["node", "npm", "npx", "corepack"];
const NODE_PROVIDER: RuntimeProvider = RuntimeProvider {
    tool: "node",
    commands: NODE_COMMANDS,
};
const PROVIDERS: &[RuntimeProvider] = &[NODE_PROVIDER];

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
    fn unavailable_providers_and_commands_are_not_invented() {
        assert!(runtime_provider("python").is_none());
        assert!(runtime_provider_for_command("python").is_none());
        assert!(runtime_provider_for_command("flutter").is_none());
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
}
