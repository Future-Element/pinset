const path=require('node:path');
const fs=require('node:fs');
const {execFileSync}=require('node:child_process');
const {downloadAndUnzipVSCode,runTests}=require('../../editors/vscode/node_modules/@vscode/test-electron');
(async()=>{
  const root=path.resolve(process.argv[2]),cli=process.env.PINSET_TEST_CLI||path.join(process.env.CARGO_TARGET_DIR||'/build/target','debug/pinset');
  const route=JSON.parse(execFileSync(cli,['-C',root,'--json','which','java'],{encoding:'utf8'}));
  const executable=await downloadAndUnzipVSCode({version:'1.112.0',platform:'linux-x64',cachePath:path.join(process.env.PINSET_ACCEPTANCE_CACHE||'/sdk-cache','vscode')});
  const result=path.join(process.env.PINSET_ACCEPTANCE_REPORTS||'/tmp/pinset-reports','editor-host.json');
  fs.mkdirSync(path.dirname(result),{recursive:true});
  fs.mkdirSync(path.join(root,'.vscode'),{recursive:true});
  fs.writeFileSync(path.join(root,'.vscode/settings.json'),'{\n  // Preserve the separate language-server JDK\n  "java.jdt.ls.java.home": "/external/language-server-jdk"\n}\n');
  await runTests({vscodeExecutablePath:executable,extensionDevelopmentPath:path.resolve('editors/vscode'),
    extensionTestsPath:path.resolve('editors/vscode/test/host-suite.cjs'),
    extensionTestsEnv:{PINSET_EDITOR_PROJECT:root,PINSET_EDITOR_CLI:cli,PINSET_EDITOR_JDK:route.sdk,PINSET_EDITOR_RESULT:result},
    launchArgs:[root,'--no-sandbox','--disable-workspace-trust','--disable-gpu','--user-data-dir',path.join(root,'.pinset/local/vscode-user'),'--extensions-dir',path.join(root,'.pinset/local/vscode-extensions')]});
  if(!fs.existsSync(result))throw new Error('real extension host produced no acceptance result');
  const nested=JSON.parse(fs.readFileSync(result,'utf8'));
  const folders=nested.bindingContexts?.folders;
  if(!Array.isArray(folders)||folders.length!==2)throw new Error('nested project binding acceptance did not prepare two independent folders');
  const workspace=path.join(root,'.pinset/local/editor-binding.code-workspace');
  fs.writeFileSync(workspace,JSON.stringify({folders:folders.map(directory=>({path:directory})),settings:{'pinset.executable':cli}}));
  // Changing a single-folder window into a multi-root workspace restarts the
  // extension host. Start its second acceptance host with the workspace ready.
  await runTests({vscodeExecutablePath:executable,extensionDevelopmentPath:path.resolve('editors/vscode'),
    extensionTestsPath:path.resolve('editors/vscode/test/binding-multi-root-host.cjs'),
    extensionTestsEnv:{PINSET_EDITOR_PROJECT:root,PINSET_EDITOR_CLI:cli,PINSET_EDITOR_JDK:route.sdk,PINSET_EDITOR_RESULT:result},
    launchArgs:[workspace,'--no-sandbox','--disable-workspace-trust','--disable-gpu','--user-data-dir',path.join(root,'.pinset/local/vscode-user-binding'),'--extensions-dir',path.join(root,'.pinset/local/vscode-extensions')]});
  if(!JSON.parse(fs.readFileSync(result,'utf8')).bindingContexts?.multi_root)throw new Error('real multi-root extension host produced no binding acceptance result');
})().catch(error=>{console.error(error);process.exitCode=1;});
