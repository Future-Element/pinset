const assert = require("node:assert/strict");
const test = require("node:test");

const { statusModel } = require("../dist/test/status-model.js");

function context(overrides = {}) {
  return {
    protocol_schema: 1,
    minimum_extension_version: "1.0.0",
    cli_version: "2.12.2",
    requires_workspace_trust: true,
    folder: "/workspace",
    project_root: "/workspace",
    config: "/workspace/pinset.toml",
    workspace_members: [],
    environment: { profiles: ["dev", "prod"], selected: "dev", source: "local" },
    tasks: [],
    diagnostics: {
      project: { configured: true, config_schema: 5, lock_schema: 4, tasks: 0 },
      tools: [
        { name: "node", requested: "lts", locked_version: "24.8.0", provider: "node", current_target_artifact: true },
        { name: "python", requested: "3.13", locked_version: "3.13.7", provider: "python", current_target_artifact: true },
      ],
      summary: { passed: true, errors: 0, warnings: 0, info: 0 },
      findings: [],
    },
    ...overrides,
  };
}

test("shows Pinset, toolchain, and environment versions for a configured project", () => {
  const model = statusModel(context());
  assert.equal(model.primary.text, "$(tools) Pinset 2.12.2");
  assert.equal(model.tools.text, "$(code) node 24.8.0 · python 3.13.7");
  assert.equal(model.environment.text, "$(server-environment) dev");
  assert.equal(model.environment.command, "pinset.selectEnvironment");
});

test("shows a one-click init action for an unconfigured folder", () => {
  const value = context({
    project_root: null,
    config: null,
    diagnostics: {
      project: { configured: false, tasks: 0 },
      tools: [],
      summary: { passed: false, errors: 2, warnings: 0, info: 0 },
      findings: [],
    },
  });
  const model = statusModel(value);
  assert.equal(model.primary.text, "$(add) Pinset: Init");
  assert.equal(model.primary.command, "pinset.initializeProject");
  assert.equal(model.tools, undefined);
  assert.equal(model.environment, undefined);
});
