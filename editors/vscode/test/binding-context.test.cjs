const test=require('node:test');
const assert=require('node:assert/strict');
const Module=require('node:module');
const path=require('node:path');

function host(roots,route){
  const callbacks=new Map(),settings=new Map(),writes=[];
  const file=p=>({scheme:'file',fsPath:p,toString:()=>p});
  const folders=roots.map(root=>({uri:file(root)}));
  const event=()=>({dispose(){}});
  class FileSystemError extends Error{constructor(){super('missing');this.code='FileNotFound';}}
  const workspaceFolder=uri=>folders.filter(folder=>uri.fsPath===folder.uri.fsPath||uri.fsPath.startsWith(folder.uri.fsPath+path.sep))
    .sort((a,b)=>b.uri.fsPath.length-a.uri.fsPath.length)[0];
  const vscode={FileSystemError,
    workspace:{isTrusted:true,workspaceFolders:folders,textDocuments:[],getWorkspaceFolder:workspaceFolder,
      getConfiguration:()=>({get:(_key,fallback)=>fallback}),
      fs:{readFile:async uri=>{if(!settings.has(uri.fsPath))throw new FileSystemError();return Buffer.from(settings.get(uri.fsPath));},
        createDirectory:async()=>{},writeFile:async(uri,data)=>{settings.set(uri.fsPath,Buffer.from(data).toString());writes.push(uri.fsPath);}},
      createFileSystemWatcher:()=>({dispose(){},onDidCreate:event,onDidChange:event,onDidDelete:event}),onDidGrantWorkspaceTrust:event},
    window:{activeTextEditor:undefined,createOutputChannel:()=>({appendLine(){},show(){},dispose(){}}),
      createStatusBarItem:()=>({show(){},dispose(){}}),showErrorMessage:async()=>{},onDidChangeActiveTextEditor:event},
    commands:{registerCommand:(name,action)=>{callbacks.set(name,action);return event();}},
    Uri:{file,joinPath:(uri,...parts)=>file(path.join(uri.fsPath,...parts))},StatusBarAlignment:{Left:1}};
  const child={execFile:(_exe,args,_options,done)=>{
    const cwd=args[args.indexOf('-C')+1];
    const result=args.includes('which')?{protocol:'pinset/3',commands:route(cwd)}:{protocol:'pinset/3',report:{checks:[]}};
    queueMicrotask(()=>done(null,JSON.stringify(result),''));return{on(){}};
  }};
  const original=Module._load;
  Module._load=function(name,...args){return name==='vscode'?vscode:name==='node:child_process'?child:original.call(this,name,...args);};
  let extension;
  try{delete require.cache[require.resolve('../dist/extension.js')];extension=require('../dist/extension.js');}
  finally{Module._load=original;}
  extension.activate({subscriptions:[]});
  return{bind:()=>callbacks.get('pinset.bind')(),activate:source=>{vscode.window.activeTextEditor={document:{uri:file(source)}};},settings,writes};
}
const python=project=>({protocol:'pinset/3',tool:'python',project,version:'3.12.15',sdk:'/sdk/python',executable:project+'/.venv/bin/python'});

test('nested project binding cannot overwrite its enclosing workspace folder',async()=>{
  const mock=host(['/repo'],cwd=>[python(cwd.startsWith('/repo/a/')?'/repo/a':'/repo/b')]);
  for(const project of ['a','b']){
    mock.activate(`/repo/${project}/src/main.py`);
    await assert.rejects(mock.bind(),/Open the project separately or add it as a workspace folder/);
  }
  assert.equal(mock.writes.length,0);
  assert.equal(mock.settings.size,0);
});

test('multi-root projects bind separate interpreters without overwriting each other',async()=>{
  const mock=host(['/repo/a','/repo/b'],cwd=>[python(cwd.startsWith('/repo/a/')?'/repo/a':'/repo/b')]);
  for(const project of ['a','b']){mock.activate(`/repo/${project}/src/main.py`);await mock.bind();}
  assert.deepEqual(mock.writes,['/repo/a/.vscode/settings.json','/repo/b/.vscode/settings.json']);
  assert.equal(JSON.parse(mock.settings.get(mock.writes[0]))['python.defaultInterpreterPath'],'/repo/a/.venv/bin/python');
  assert.equal(JSON.parse(mock.settings.get(mock.writes[1]))['python.defaultInterpreterPath'],'/repo/b/.venv/bin/python');
});

test('global Flutter binding uses the SDK root instead of a project-local link',async()=>{
  const mock=host(['/repo'],()=>[{protocol:'pinset/3',tool:'flutter',project:null,version:'3.35.4',sdk:'/global/flutter',executable:'/global/flutter/bin/flutter'}]);
  await mock.bind();
  assert.equal(JSON.parse(mock.settings.get('/repo/.vscode/settings.json'))['dart.flutterSdkPath'],'/global/flutter');
});

test('all route contexts are validated before the first setting write',async()=>{
  const mock=host(['/repo'],()=>[python('/repo'),{protocol:'pinset/3',tool:'java',project:'/repo/nested',version:'21.0.12+1',sdk:'/jdk',executable:'/jdk/bin/java'}]);
  await assert.rejects(mock.bind(),/is not the VS Code workspace folder/);
  assert.equal(mock.writes.length,0);
});

test('Windows canonical namespace and case match the workspace folder',async()=>{
  const descriptor=Object.getOwnPropertyDescriptor(process,'platform');
  Object.defineProperty(process,'platform',{value:'win32',configurable:true});
  try{
    const root='C:\\Repo';
    const mock=host([root],()=>[python('\\\\?\\c:\\repo')]);
    await mock.bind();
    assert.equal(mock.writes.length,1);
    assert.equal(mock.writes[0],path.join(root,'.vscode','settings.json'));
  }finally{Object.defineProperty(process,'platform',descriptor);}
});
