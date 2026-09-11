import type { PinsetContext } from "./protocol";

export interface StatusItemModel {
  text: string;
  tooltip: string;
  command: string;
}

export interface StatusModel {
  primary: StatusItemModel;
  tools?: StatusItemModel;
  environment?: StatusItemModel;
}

export function statusModel(context: PinsetContext): StatusModel {
  if (!context.config || !context.diagnostics.project.configured) {
    return {
      primary: {
        text: "$(add) Pinset: Init",
        tooltip: `Initialize Pinset in ${context.folder}\nCreates pinset.toml by running pinset init.`,
        command: "pinset.initializeProject",
      },
    };
  }

  const summary = context.diagnostics.summary;
  const icon = summary.errors > 0 ? "error" : summary.warnings > 0 ? "warning" : "tools";
  const tools = context.diagnostics.tools;
  const visibleTools = tools.slice(0, 3).map((tool) => `${tool.name} ${tool.locked_version ?? tool.requested}`);
  const additionalTools = tools.length > visibleTools.length ? ` · +${tools.length - visibleTools.length}` : "";
  const toolsText = visibleTools.length > 0 ? visibleTools.join(" · ") + additionalTools : "No toolchains";
  const toolsTooltip = tools.length > 0
    ? tools
        .map((tool) => {
          const version = tool.locked_version ?? tool.requested;
          const requested = tool.locked_version && tool.locked_version !== tool.requested ? `, requested ${tool.requested}` : "";
          const provider = tool.provider ? `, provider ${tool.provider}` : "";
          return `${tool.name}: ${version}${requested}${provider}`;
        })
        .join("\n")
    : "No toolchains are declared in pinset.toml.";
  const profile = context.environment.selected ?? "default";
  const profiles = context.environment.profiles.length > 0 ? context.environment.profiles.join(", ") : "none";

  return {
    primary: {
      text: `$(${icon}) Pinset ${context.cli_version}`,
      tooltip: `Pinset CLI ${context.cli_version}\n${context.project_root ?? context.folder}\n${summary.errors} error(s), ${summary.warnings} warning(s)`,
      command: "pinset.checkDiagnostics",
    },
    tools: {
      text: `$(code) ${toolsText}`,
      tooltip: toolsTooltip,
      command: "pinset.checkDiagnostics",
    },
    environment: {
      text: `$(server-environment) ${profile}`,
      tooltip: `Environment: ${profile}\nSelection source: ${context.environment.source}\nAvailable profiles: ${profiles}`,
      command: "pinset.selectEnvironment",
    },
  };
}
