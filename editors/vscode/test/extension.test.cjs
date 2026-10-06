const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const path = require('node:path');

test('trust gates processes, binding keeps language-server JDK separate, commands use v3', async () => {
  const callbacks = new Map(), updates = [], errors = [], calls = [];
  let trusted = false;
  let settingsText='{\n // preserve comments\n "java.jdt.ls.java.home":"/external/server-jdk"\n}';
  const file=p=>({scheme:"file",fsPath:p,toString:()=>p});
  const configuration = { 'pinset.executable': '/verified/pinset', 'java.configuration.runtimes': [{ name:'JavaSE-17', path:'/older/jdk',default:true }] };
  const event = () => ({ dispose() {} });
  const folder={uri:file('/project')};
  const vscode = {
    workspace:{get isTrusted(){return trusted;},workspaceFolders:[folder],getWorkspaceFolder:()=>folder,
      getConfiguration:section=>({get:(key,fallback)=>configuration[section+'.'+key]??fallback,
        update:async (key,value)=>updates.push([section+'.'+key,value])}),
      textDocuments:[],fs:{readFile:async()=>Buffer.from(settingsText),createDirectory:async()=>{},writeFile:async (_uri,data)=>{settingsText=Buffer.from(data).toString();}},
      createFileSystemWatcher:()=>({dispose(){},onDidCreate:event,onDidChange:event,onDidDelete:event}),onDidGrantWorkspaceTrust:event},
    window:{activeTextEditor:undefined,createOutputChannel:()=>({appendLine(){},show(){},dispose(){}}),
      createStatusBarItem:()=>({show(){},dispose(){}}),showErrorMessage:async e=>errors.push(e),onDidChangeActiveTextEditor:event},
    commands:{registerCommand:(name,action)=>{callbacks.set(name,action);return event();}},
    Uri:{file,joinPath:(uri,...parts)=>file(path.join(uri.fsPath,...parts))},StatusBarAlignment:{Left:1},ConfigurationTarget:{Workspace:2,WorkspaceFolder:3},
  };
  const child={execFile:(exe,args,options,done)=>{
    calls.push({exe,args,options});
    const value=args.includes('which')?{protocol:'pinset/3',commands:[{protocol:'pinset/3',tool:'java',sdk:'/locked/jdk',version:'21.0.12+1',project:'/project',executable:'/locked/jdk/bin/java'},
      {protocol:'pinset/3',tool:'flutter',sdk:'/locked/flutter',project:'/project',version:'3.35.4',executable:'/locked/flutter/bin/flutter'}]}
      :{protocol:'pinset/3',report:{checks:[{tool:'java',installed:true,bound:true,actually_verified:false,detail:'bound'}]}};
    queueMicrotask(()=>done(null,JSON.stringify(value),''));return {on(){}};
  }};
  const original=Module._load;
  Module._load=function(name,...args){return name==='vscode'?vscode:name==='node:child_process'?child:original.call(this,name,...args);};
  let extension;
  try{delete require.cache[require.resolve('../dist/extension.js')];extension=require('../dist/extension.js');}
  finally{Module._load=original;}
  extension.activate({subscriptions:[]});
  await assert.rejects(callbacks.get('pinset.check')(),/Workspace Trust/);assert.equal(calls.length,0);assert.equal(errors.length,1);
  trusted=true;await callbacks.get('pinset.bind')();
  assert.equal(callbacks.size,5);
  const settings=require('jsonc-parser').parse(settingsText);
  assert(settingsText.includes('// preserve comments'));
  assert.equal(settings['java.jdt.ls.java.home'],'/external/server-jdk');
  const runtimes=settings['java.configuration.runtimes'];
  assert.deepEqual(runtimes,[{name:'JavaSE-17',path:'/older/jdk',default:false},{name:'JavaSE-21',path:'/locked/jdk',default:true}]);
  assert.equal(settings['dart.flutterSdkPath'],path.join('/project','.pinset/local/flutter-sdk'));
  assert(!updates.some(([key])=>key.includes('jdt.ls.java.home')));
  const saved=settingsText;
  vscode.workspace.textDocuments.push({uri:file('/project/.vscode/settings.json'),isDirty:true});
  await assert.rejects(callbacks.get('pinset.bind')(),/Save project settings/);
  assert.equal(settingsText,saved);vscode.workspace.textDocuments.pop();
  settingsText='{invalid';
  await assert.rejects(callbacks.get('pinset.bind')(),/valid JSON object/);
  assert.equal(settingsText,'{invalid');settingsText=saved;
  await callbacks.get('pinset.install')();
  assert(calls.some(({args,options})=>args.includes('install')&&options.timeout===1_800_000));
  assert(calls.every(({args,options})=>args.includes('--json')&&args.includes('-C')&&options.windowsHide===true));
});
