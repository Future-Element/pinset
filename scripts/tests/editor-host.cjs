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
})().catch(error=>{console.error(error);process.exitCode=1;});
