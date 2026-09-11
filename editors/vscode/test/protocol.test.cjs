const assert = require("node:assert/strict");
const test = require("node:test");

const { parseContext } = require("../dist/test/protocol.js");

function envelope(protocol = 1, minimum = "1.0.0") {
  return JSON.stringify({
    schema: 1,
    command: "editor.context",
    ok: true,
    data: {
      protocol_schema: protocol,
      minimum_extension_version: minimum,
      cli_version: "2.11.0",
      requires_workspace_trust: true,
      folder: "/workspace",
      workspace_members: [],
      environment: { profiles: [], source: "none" },
      tasks: [],
      diagnostics: {
        project: { configured: false, tasks: 0 },
        tools: [],
        summary: { passed: true, errors: 0, warnings: 0, info: 0 },
        findings: [],
      },
    },
  });
}

test("accepts the supported editor protocol", () => {
  assert.equal(parseContext(envelope(), "1.0.0").cli_version, "2.11.0");
});

test("rejects incompatible protocol and extension versions", () => {
  assert.throws(() => parseContext(envelope(2), "1.0.0"), /protocol 2/);
  assert.throws(() => parseContext(envelope(1, "1.1.0"), "1.0.0"), /requires extension 1.1.0/);
});

test("rejects non-protocol output", () => {
  assert.throws(() => parseContext("not json", "1.0.0"), /invalid JSON/);
});

test("rejects a malformed payload within a supported envelope", () => {
  const value = JSON.parse(envelope());
  delete value.data.tasks;
  assert.throws(() => parseContext(JSON.stringify(value), "1.0.0"), /invalid editor context payload/);
});
