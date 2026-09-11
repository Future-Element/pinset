const assert = require("node:assert/strict");
const test = require("node:test");

const { diagnosticDocument } = require("../dist/test/diagnostic-model.js");

test("routes source-backed findings to their owning Pinset file", () => {
  assert.equal(diagnosticDocument("configuration"), "config");
  assert.equal(diagnosticDocument("lock"), "lock");
  assert.equal(diagnosticDocument("platform_artifact"), "lock");
  assert.equal(diagnosticDocument("provenance"), "lock");
});

test("keeps operational health findings out of file diagnostics", () => {
  for (const category of ["install_receipt", "ownership", "cache"]) {
    assert.equal(diagnosticDocument(category), undefined);
  }
});
