const assert = require("node:assert/strict");
const test = require("node:test");

const { isMissingExecutableError, processStartError } = require("../dist/test/process-error.js");

test("classifies ENOENT as a missing Pinset executable", () => {
  const error = processStartError(Object.assign(new Error("spawn pinset ENOENT"), { code: "ENOENT" }));
  assert.equal(error.kind, "missing-executable");
  assert.equal(isMissingExecutableError(error), true);
});

test("does not classify other spawn failures as a missing executable", () => {
  const error = processStartError(Object.assign(new Error("spawn pinset EACCES"), { code: "EACCES" }));
  assert.equal(error.kind, "cli");
  assert.equal(isMissingExecutableError(error), false);
});
