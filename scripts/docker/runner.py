import datetime,hashlib,json,os,shutil,subprocess,sys,time,traceback
from pathlib import Path
sys.path.insert(0,'/source/scripts')
from release_gate import source_fingerprint
from verification_policy import LARGE_ARTIFACT_BOUNDARY, RUNTIME_EXEMPTIONS

source=Path('/source');workspace=Path('/workspace');reports=Path('/reports')/('verify-v3-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'))
suite=sys.argv[1]
if suite not in ['fast','acceptance','platform','integrations','all']:raise SystemExit('unknown suite')
# A reused container keeps caches, but each run gets fresh local state and credentials.
if workspace.is_symlink() or workspace.is_mount():raise SystemExit('workspace must be a private container directory')
local=Path('/run/pinset-verification')
if local.is_symlink() or local.is_mount():raise SystemExit('unsafe verification state directory')
if local.exists():shutil.rmtree(local)
local.mkdir()
for directory in ['home','tmp','xdg-data','xdg-config']:(local/directory).mkdir(mode=0o700)
home=Path('/run/pinset')
if home.is_symlink():raise SystemExit('unsafe test Pinset home')
if home.exists():
    for entry in home.iterdir():
        if entry.name=='v3' and entry.is_dir() and not entry.is_symlink():
            for child in entry.iterdir():
                if child.name=='cache':continue
                if child.is_dir() and not child.is_symlink():shutil.rmtree(child)
                else:child.unlink()
        elif entry.is_dir() and not entry.is_symlink():shutil.rmtree(entry)
        else:entry.unlink()
reports.mkdir(parents=True,exist_ok=False)
start_commit=subprocess.check_output(['git','-c','safe.directory=/source','rev-parse','HEAD'],cwd=source,text=True).strip()
start_fingerprint=source_fingerprint(source)
start_lock=hashlib.sha256((source/'Cargo.lock').read_bytes()).hexdigest()
def ignore(directory,names):
    relative=Path(directory).relative_to(source)
    excluded={'node_modules','.git','.next','__pycache__'}
    if not relative.parts:excluded.update({'output','target','plan','.pnpm-store'})
    if str(relative)=='.pinset':excluded.update({'acceptance','local'})
    if str(relative)=='editors/vscode':excluded.add('dist')
    if str(relative)=='website':excluded.add('out')
    return names.intersection(excluded) if isinstance(names,set) else set(names)&excluded
if workspace.exists():shutil.rmtree(workspace)
shutil.copytree(source,workspace,ignore=ignore)
os.chdir(workspace)
os.environ.update({'PINSET_HOME':str(home),'HOME':str(local/'home'),'TMPDIR':str(local/'tmp'),'XDG_DATA_HOME':str(local/'xdg-data'),'XDG_CONFIG_HOME':str(local/'xdg-config'),'DONT_PROMPT_WSL_INSTALL':'1','CARGO_TARGET_DIR':'/build/target','PINSET_ACCEPTANCE_REPORTS':str(reports),'PINSET_ACCEPTANCE_CACHE':'/sdk-cache'})
for name in ['PINSET_IDENTITY','PINSET_PROFILE','PINSET_NO_ENV','PINSET_ENV_RESOLVED']:os.environ.pop(name,None)
commands=[];results={}
def snapshot_binaries():
    directory=Path('/run/pinset-test-bin');directory.mkdir(exist_ok=True)
    for name in ['pinset','pinset-shim']:shutil.copy2(Path('/build/target/debug')/name,directory/name)
    os.environ['PINSET_TEST_CLI']=str(directory/'pinset')
def run(args,label,timeout=3600,env=None):
    start=time.monotonic();filename=reports/(label+'.log');print('RUN',label,flush=True)
    command={'label':label,'argv':args,'cwd':str(Path.cwd())}
    with filename.open('wb') as output:
        try:
            process=subprocess.run(args,stdout=output,stderr=subprocess.STDOUT,timeout=timeout,env=env)
            code=process.returncode
        except subprocess.TimeoutExpired:code=124
    command.update({'exit_code':code,'seconds':round(time.monotonic()-start,2),'log':filename.name});commands.append(command)
    if code:
        print(filename.read_text(errors='replace')[-12000:],flush=True);raise RuntimeError(f'{label} failed with exit {code}')
    print('PASS',label,flush=True)
def fast():
    run(['cargo','fmt','--all','--','--check'],'format')
    run(['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings'],'lint')
    run(['cargo','build','--workspace','--bins','--locked'],'binaries')
    snapshot_binaries()
    run(['cargo','test','--workspace','--locked'],'rust-contracts')
    run(['python3','scripts/security_audit.py'],'cargo-security')
    run(['python3','scripts/tests/contracts.py'],'application-contracts')
    run(['python3','scripts/tests/surfaces.py'],'distribution-contracts')
def acceptance():
    run(['cargo','build','--workspace','--bins','--locked'],'acceptance-binaries')
    snapshot_binaries()
    run(['python3','scripts/tests/acceptance.py'],'real-sdk-acceptance',timeout=14_400)
def platform():
    run(['bash','scripts/tests/platform.sh'],'platform',timeout=7200)
def integrations():
    run(['cargo','build','--workspace','--bins','--locked'],'integration-binaries')
    snapshot_binaries()
    run(['npm','ci','--ignore-scripts','--prefix','editors/vscode'],'extension-dependencies')
    run(['npm','run','typecheck','--prefix','editors/vscode'],'extension-types')
    run(['npm','audit','--audit-level=high','--prefix','editors/vscode'],'extension-security')
    run(['npm','run','test','--prefix','editors/vscode'],'extension-unit')
    run(['npm','run','package','--prefix','editors/vscode'],'extension-package')
    run(['pnpm','--dir','website','install','--frozen-lockfile'],'website-dependencies')
    run(['pnpm','--dir','website','typecheck'],'website-types')
    run(['pnpm','--dir','website','audit','--audit-level','high'],'website-security')
    run(['pnpm','--dir','website','build'],'website-package')
    run(['pnpm','--dir','website','validate:seo'],'website-seo')
    run(['dbus-run-session','--','bash','scripts/tests/integrations.sh'],'runtime-integrations',timeout=14_400)
functions={'fast':fast,'acceptance':acceptance,'platform':platform,'integrations':integrations}
try:
    for name in functions if suite=='all' else [suite]:
        start=time.monotonic()
        try:functions[name]();results[name]={'status':'passed','seconds':round(time.monotonic()-start,2)}
        except Exception as error:results[name]={'status':'failed','seconds':round(time.monotonic()-start,2),'error':str(error)};traceback.print_exc()
finally:
    commit=start_commit
    if source_fingerprint(source)!=start_fingerprint:
        results['source']={'status':'failed','error':'source changed during verification'}
    changes=subprocess.check_output(['git','-c','safe.directory=/source','status','--porcelain','--untracked-files=all'],cwd=source,text=True)
    changes=[line for line in changes.splitlines() if not line[3:].startswith(('output/','.pinset/'))]
    image=os.environ.get('PINSET_VERIFY_IMAGE_DIGEST','')
    report={'protocol':'pinset-verification/3','execution':'local-docker','container_id':os.environ.get('PINSET_VERIFY_CONTAINER_ID',''),'container_reused':True,'suite':suite,'commit':commit,'source_clean':not changes,'source_fingerprint':start_fingerprint,'lock_sha256':start_lock,'image_digests':{'verify':image,'rust':'sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3','node':'sha256:2fe369e969550cde8e867afc3fe370b260140cab4a23d467074295b42163d553'},'commands':commands,'suites':results,'unverified':['Windows x86_64 native runtime; only target compilation and path/credential contracts','macOS aarch64 native runtime; only target compilation and path/credential contracts'],'arm64_execution':'QEMU emulation; no native ARM64 host'}
    required={'fast':['contracts.json','distribution.json','cargo-security.json'],
      'acceptance':['real-sdks.json','frozen-selectors.json'],
      'platform':['platform.json','frozen-selectors.json'],
      'integrations':['integrations.json','editor-host.json','flutter-contracts.json','frozen-selectors.json']}
    report['runtime_exemptions']=RUNTIME_EXEMPTIONS
    report['unverified'].append(LARGE_ARTIFACT_BOUNDARY)
    report['evidence_files']={}
    for name,result in list(results.items()):
        if name not in required or result['status']!='passed':continue
        for filename in required[name]:
            path=reports/filename
            if not path.is_file():
                results[name]={'status':'failed','error':'missing required report: '+filename}
                continue
            content=path.read_bytes()
            report['evidence_files'][filename]={'sha256':hashlib.sha256(content).hexdigest(),'bytes':len(content)}
    selections=reports/'frozen-selectors.json'
    if selections.is_file():report['sdk_selections']=json.loads(selections.read_text())
    security=reports/'cargo-security.json'
    if security.is_file():report['security_applicability_exceptions']=json.loads(security.read_text()).get('applicability_exceptions',[])
    raw=json.dumps(report,sort_keys=True,separators=(',',':')).encode();(reports/'report.json').write_bytes(raw);(reports/'report.sha256').write_text(hashlib.sha256(raw).hexdigest()+'\n');print('REPORT',reports/'report.json',flush=True)
raise SystemExit(0 if all(r['status']=='passed' for r in results.values()) and len(results)==(4 if suite=='all' else 1) else 1)
