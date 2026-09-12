const assert = require("node:assert/strict");
const vscode = require("vscode");
exports.run = async () => {
  assert.equal(vscode.workspace.isTrusted, true);
  const extension = vscode.extensions.getExtension("FutureElement.pinset-vscode");
  assert.ok(extension);
  await extension.activate();
  await vscode.commands.executeCommand("pinset.refresh");
  for (const tool of ["node", "python"]) {
    const evidence = await vscode.commands.executeCommand("pinset.probeDebug", tool);
    assert.equal(evidence?.entry, "pinset-debug-probe", JSON.stringify(evidence));
    assert.equal(evidence?.state, "pass", JSON.stringify(evidence));
    console.log(`Native VS Code ${tool} debug evidence: ${JSON.stringify(evidence)}`);
  }
};
