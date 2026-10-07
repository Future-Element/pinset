"""Install/archive/Action/release metadata contracts, entirely inside local Docker."""
import functools, hashlib, http.server, json, os, subprocess, tempfile, threading, zipfile
from pathlib import Path
from support import BIN, run, report

root=Path.cwd();fixture=Path(tempfile.mkdtemp(prefix='pinset-distribution-'))
version=__import__('tomllib').loads((root/'Cargo.toml').read_text())['workspace']['package']['version']
tag='v'+version
archive=fixture/f'pinset-{tag}-linux-x86_64.zip'
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as target:
    for name in ['pinset','pinset-shim']:
        info=zipfile.ZipInfo(name);info.create_system=3;info.external_attr=0o100755<<16
        target.writestr(info,(BIN.parent/name).read_bytes())
(fixture/'SHA256SUMS').write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+'  '+archive.name+'\n')
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(http.server.SimpleHTTPRequestHandler,directory=str(fixture)))
threading.Thread(target=server.serve_forever,daemon=True).start()
home=fixture/'home';legacy=home/'installs';legacy.mkdir(parents=True);(legacy/'preserve').write_text('old data')
env={**os.environ,'HOME':str(fixture),'PINSET_HOME':str(home),'PINSET_INSTALL_TEST_MODE':'1',
     'PINSET_TEST_RELEASE_BASE_URL':f'http://127.0.0.1:{server.server_port}'}
try:
    run(['bash','install.sh'],env=env)
    assert (home/'v3/.pinset-home.json').exists()
    assert 'pinset '+version in run([home/'v3/bin/pinset','--version'],env=env)
    assert (home/'v3/bin/javac').is_file()
    before=(home/'v3/bin/pinset').read_bytes()
    (fixture/'SHA256SUMS').write_text('0'*64+'  '+archive.name+'\n')
    run(['bash','install.sh'],env=env,expected=1)
    assert (home/'v3/bin/pinset').read_bytes()==before
    run(['bash','uninstall.sh','--plan'],env=env)
    assert (home/'v3/bin/pinset').exists()
    run(['bash','uninstall.sh','--yes'],env=env)
    assert not (home/'v3').exists() and (legacy/'preserve').read_text()=='old data'
    external=fixture/'external/v3';external.mkdir(parents=True);(external/'preserve').write_text('outside')
    run(['bash','uninstall.sh','--yes','--pinset-home',external.parent],env=env,expected=1)
    assert (external/'preserve').exists()
    # A custom CLI directory can contain obsolete launchers. The install hint
    # must prepend the actual v3 entry directory, preserving unrelated files.
    (fixture/'SHA256SUMS').write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+'  '+archive.name+'\n')
    custom=fixture/'custom binaries';custom.mkdir()
    obsolete=custom/'bun';obsolete.write_text('#!/bin/sh\nprintf obsolete-launcher\nexit 97\n');obsolete.chmod(0o755)
    custom_home=fixture/"custom data's home"
    custom_env={**env,'PINSET_HOME':str(custom_home),'PATH':str(custom)+':'+env['PATH']}
    installed=run(['bash','install.sh','--install-dir',custom],env=custom_env)
    assert obsolete.read_text()=='#!/bin/sh\nprintf obsolete-launcher\nexit 97\n'
    integration=run([custom/'pinset','self','shell','bash'],env=custom_env).strip()
    assert integration in installed
    managed=custom_home/'v3/bin'
    assert 'Provider command entries are in '+str(managed) in installed
    discovered=run(['bash','-c',integration+'\ncommand -v pinset\ncommand -v bun'],env=custom_env)
    assert discovered.splitlines()==[str(managed/'pinset'),str(managed/'bun')],discovered
    routed=run(['bash','-c',integration+'\nbun -v'],env=custom_env,expected=1)
    assert 'PINSET_SELECTION_MISSING' in routed and 'PINSET_COMMAND_INVALID' not in routed,routed
finally:server.shutdown()

# Package hooks must never call independent validation or use lifecycle hooks to hide it.
for directory in ['editors/vscode','website']:
    package=json.loads((root/directory/'package.json').read_text())
    for name,command in package['scripts'].items():
        if name in ['package','build','prepackage','prebuild','vscode:prepublish','prepare']:
            assert not any(word in command for word in [' test','typecheck',' audit',' lint','verify.sh','verify.ps1']),command
workflow=(root/'.github/workflows/release.yml').read_text()
assert 'workflow_dispatch:' in workflow
for forbidden in ['cargo test','cargo clippy','cargo audit','npm test','npm audit','typecheck','verify.sh','release-preflight']:
    assert forbidden not in workflow,forbidden
assert list((root/'.github/workflows').glob('*.yml'))==[root/'.github/workflows/release.yml']
assert workflow.index('--draft --latest=false') < workflow.index('python3 scripts/check_release_assets.py') < workflow.index('--draft=false --latest=')
action=(root/'action.yml').read_text()
assert 'pinset -C "$PINSET_PROJECT_DIRECTORY" install' in action
assert all(old not in action for old in ['upgrade prepare','env trust','cache:','task','workspace:'])
assert '--body-file' not in action
english=(root/'docs/commands.md').read_text();chinese=(root/'docs/commands.zh-CN.md').read_text()
import re
assert re.findall(r'^### `([^`]+)`',english,re.M)==re.findall(r'^### `([^`]+)`',chinese,re.M)
sysroot=str(root/'scripts');import sys;sys.path.insert(0,sysroot)
from release_gate import validate, source_fingerprint
from check_release_assets import validate_assets
from verification_policy import LARGE_ARTIFACT_BOUNDARY, RUNTIME_EXEMPTIONS
try:validate({'protocol':'pinset-verification/3','suite':'all','execution':'CI'},root,tag)
except ValueError:pass
else:raise AssertionError('release gate accepted non-local report')
# Synthetic metadata tests exercise the release gate locally; they never publish.
source=Path('/source')
commit=subprocess.check_output(['git','-c','safe.directory=/source','rev-parse','HEAD'],cwd=source,text=True).strip()
metadata={'protocol':'pinset-verification/3','suite':'all','execution':'local-docker','source_clean':True,
  'commit':commit,'source_fingerprint':source_fingerprint(source),'lock_sha256':hashlib.sha256((source/'Cargo.lock').read_bytes()).hexdigest(),
  'suites':{name:{'status':'passed'} for name in ['fast','acceptance','platform','integrations']},
  'commands':[{'exit_code':0}], 'image_digests':{'verify':'sha256:'+'1'*64},
  'evidence_files':{name:{'sha256':'2'*64,'bytes':1} for name in ['contracts.json','distribution.json','cargo-security.json','real-sdks.json','platform.json','integrations.json','editor-host.json','flutter-contracts.json','frozen-selectors.json']},
  'arm64_execution':'QEMU emulation','unverified':['Windows native runtime','macOS native runtime',LARGE_ARTIFACT_BOUNDARY],
  'runtime_exemptions':RUNTIME_EXEMPTIONS}
assert validate(metadata,source,tag)==commit
for field,value in [('source_clean',False),('commit','0'*40),('source_fingerprint','0'*64),('runtime_exemptions',[]),('unverified',['Windows native runtime','macOS native runtime'])]:
    invalid={**metadata,field:value}
    try:validate(invalid,source,tag)
    except ValueError:pass
    else:raise AssertionError('release gate accepted invalid '+field)
# Publication only checks already-built artifacts and server metadata. Exercise
# incomplete/damaged uploads locally; these cases never invoke GitHub or CI.
draft_dir=fixture/'draft-assets';draft_dir.mkdir()
names=['pinset-vscode.vsix','local-verification.json']
for platform in ['linux-x86_64','linux-aarch64','windows-x86_64','macos-aarch64']:
    names.extend([f'pinset-{tag}-{platform}.zip',f'pinset-{platform}.sbom.json'])
for name in names:(draft_dir/name).write_bytes(('fixture-'+name).encode())
(draft_dir/'SHA256SUMS').write_text(''.join(hashlib.sha256((draft_dir/name).read_bytes()).hexdigest()+'  '+name+'\n' for name in names))
draft={'tag_name':tag,'draft':True,'prerelease':'-' in tag,'assets':[
    {'name':path.name,'state':'uploaded','size':path.stat().st_size,'digest':'sha256:'+hashlib.sha256(path.read_bytes()).hexdigest()}
    for path in draft_dir.iterdir()]}
assert validate_assets(draft,draft_dir,tag)['complete_assets']==11
import copy
for failure in ['missing','duplicate','incomplete','size','digest','published','wrong-tag','stability']:
    damaged=copy.deepcopy(draft)
    if failure=='missing':damaged['assets'].pop()
    elif failure=='duplicate':damaged['assets'][-1]=damaged['assets'][0]
    elif failure=='incomplete':damaged['assets'][0]['state']='new'
    elif failure=='size':damaged['assets'][0]['size']+=1
    elif failure=='digest':damaged['assets'][0]['digest']='sha256:'+'0'*64
    elif failure=='published':damaged['draft']=False
    elif failure=='wrong-tag':damaged['tag_name']='v3.99.0'
    elif failure=='stability':damaged['prerelease']=not damaged['prerelease']
    try:validate_assets(damaged,draft_dir,tag)
    except ValueError:pass
    else:raise AssertionError('publication accepted '+failure+' release metadata')
from self_update import verify_self_update
from progress import verify_progress
progress = verify_progress(fixture)
updates = verify_self_update(root, fixture, version)
report('distribution',installer='real fixture binaries with checksum verification',custom_install_path='managed v3 entries precede obsolete launchers; spaces and apostrophes preserved',old_data='retained',progress=progress,self_update=updates)
