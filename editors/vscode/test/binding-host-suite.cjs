const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const vscode=require('vscode');
const {parse}=require('jsonc-parser');

exports.run=async function(root,cli,jdk){
  const parentSettings=path.join(root,'.vscode/settings.json');
  const before=fs.readFileSync(parentSettings,'utf8');
  const projects=[];
  for(const name of ['a','b']){
    const project=path.join(root,'.pinset/local/editor-binding-context',name);
    const identity='editor-binding-'+name+'-'+crypto.randomUUID();
    fs.mkdirSync(path.join(project,'.pinset'),{recursive:true});
    // A repository boundary permits project discovery from its source folder.
    fs.mkdirSync(path.join(project,'.git'));
    for(const filename of ['config.toml','lock.toml']){
      const original=fs.readFileSync(path.join(root,'.pinset',filename),'utf8');
      const copied=original.replace(/^project_id = "[^"]+"$/m,`project_id = "${identity}"`);
      assert.notEqual(copied,original,'fixture must have an independent project identity');
      fs.writeFileSync(path.join(project,'.pinset',filename),copied);
    }
    fs.mkdirSync(path.join(project,'src'));
    fs.writeFileSync(path.join(project,'src/Main.java'),'class Main {}\n');
    fs.mkdirSync(path.join(project,'.vscode'));
    fs.writeFileSync(path.join(project,'.vscode/settings.json'),JSON.stringify({'java.jdt.ls.java.home':'/external/server-'+name}));
    const document=await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(project,'src/Main.java')));
    await vscode.window.showTextDocument(document,{preview:false});
    await assert.rejects(vscode.commands.executeCommand('pinset.bind'),/Open the project separately or add it as a workspace folder/);
    assert.equal(fs.readFileSync(parentSettings,'utf8'),before,'nested project must not change enclosing settings');
    projects.push({project,document,name});
  }
  return{host:'real-vscode-extension-host',cli:'real Pinset with independent project configs and locks',nested_projects:'A and B refused before folder-level writes',folders:projects.map(({project})=>project)};
};

exports.runMultiRoot=async function(root,cli,jdk){
  const projects=[];
  for(const name of ['a','b']){
    const project=path.join(root,'.pinset/local/editor-binding-context',name);
    const document=await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(project,'src/Main.java')));
    projects.push({project,document,name});
  }
  await vscode.workspace.getConfiguration('pinset').update('executable',cli,vscode.ConfigurationTarget.Workspace);
  for(const {project,document,name} of projects){
    await vscode.window.showTextDocument(document,{preview:false});
    assert.equal(vscode.workspace.getWorkspaceFolder(document.uri).uri.fsPath,project);
    await vscode.commands.executeCommand('pinset.bind');
    const settings=parse(fs.readFileSync(path.join(project,'.vscode/settings.json'),'utf8'));
    assert.equal(settings['java.jdt.ls.java.home'],'/external/server-'+name);
    assert(settings['java.configuration.runtimes'].some(runtime=>runtime.path===jdk&&runtime.default===true));
  }
  const first=projects[0];
  const preserved=parse(fs.readFileSync(path.join(first.project,'.vscode/settings.json'),'utf8'));
  assert.equal(preserved['java.jdt.ls.java.home'],'/external/server-a','binding B must preserve A settings');
  assert(preserved['java.configuration.runtimes'].some(runtime=>runtime.path===jdk&&runtime.default===true));
  return{host:'real-vscode-extension-host',cli:'real Pinset with independent project configs and locks',multi_root:'A and B bound separately; A preserved after B',folders:projects.map(({project})=>project)};
};
