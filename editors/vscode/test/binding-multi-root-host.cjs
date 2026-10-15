const assert=require('node:assert/strict');
const fs=require('node:fs');
const vscode=require('vscode');

exports.run=async function(){
  assert(vscode.workspace.isTrusted,'test host must have explicit Workspace Trust');
  const extension=vscode.extensions.getExtension('FutureElement.pinset-vscode');
  assert(extension);await extension.activate();
  const multiRoot=await require('./binding-host-suite.cjs').runMultiRoot(process.env.PINSET_EDITOR_PROJECT,process.env.PINSET_EDITOR_CLI,process.env.PINSET_EDITOR_JDK);
  const result=JSON.parse(fs.readFileSync(process.env.PINSET_EDITOR_RESULT,'utf8'));
  result.bindingContexts={...result.bindingContexts,...multiRoot};
  fs.writeFileSync(process.env.PINSET_EDITOR_RESULT,JSON.stringify(result));
};
