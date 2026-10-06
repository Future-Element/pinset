"""Deterministic public command, project boundary and read-only contracts."""
import hashlib, json, os, subprocess, tempfile
from pathlib import Path
from support import BIN, cli, data, project, run, report

home = Path(tempfile.mkdtemp(prefix='pinset-contract-home-'))
os.environ['PINSET_HOME'] = str(home)
os.environ['HOME'] = str(home.parent)
root = project('contracts')
help_text = run([BIN, '--help'])
commands = [line.split()[0] for line in help_text.split('Commands:')[1].split('Options:')[0].splitlines() if line.startswith('  ')]
assert commands == ['init','use','remove','install','list','which','check','exec','env','clean','self'], commands
for obsolete in ['upgrade','task','workspace','doctor','setup','providers','bundle','migrate','shim','__env-resolve']:
    assert subprocess.run([BIN, obsolete, '--help'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode != 0
assert data(root, 'use', 'java', expected=1)['error']['code'] == 'PINSET_SELECTOR_REQUIRED'
assert data(root, 'use', 'java@21', 'java@17', expected=1)['error']['code'] == 'PINSET_SELECTION_DUPLICATE'
assert data(root, 'use', 'dotnet@8', expected=1)['error']['code'] == 'PINSET_TOOL_UNKNOWN'
assert data(root, 'exec','--','echo','hello',expected=1)['error']['code'] == 'PINSET_NATIVE_OUTPUT'
assert subprocess.run([BIN,'-C',root,'remove','java','--remove'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode != 0
assert data(root,'remove','java','--remove',expected=2)['error']['code']=='PINSET_ARGUMENT_INVALID'
def fingerprint():
    return [(str(p),hashlib.sha256(p.read_bytes()).hexdigest()) for base in [root,home] for p in sorted(base.rglob('*')) if p.is_file()]
before = fingerprint()
check = data(root,'check')
assert check['report']['checks'] == [] and 'user_command_verified' not in check
data(root,'install','--plan');data(root,'remove','java','--plan');data(root,'clean','cache','--plan');data(root,'self','repair','--plan')
assert fingerprint() == before, 'read-only checks or previews wrote state'
assert '/.pinset/local/' in (root/'.gitignore').read_text()
assert '/.pinset/' not in (root/'.gitignore').read_text().splitlines()
child = root/'nested';child.mkdir()
assert data(child,'check')['report']['project'] == str(root)
cli(child,'init')
assert data(child,'check')['report']['project'] == str(child)
legacy = project('legacy')
config = legacy/'.pinset/config.toml'
config.write_text('schema = 2\nnode = "lts"\n')
assert 'error' in data(legacy,'check',expected=1)
config.write_text((root/'.pinset/config.toml').read_text()+'unknown_field = true\n')
assert 'error' in data(legacy,'check',expected=1)
config.write_text((root/'.pinset/config.toml').read_text()+'\n[verification]\ntimeout = 300\n')
assert 'error' in data(legacy,'check',expected=1)
assert data(root,'use','java@21','--verify',expected=2)['error']['code']=='PINSET_ARGUMENT_INVALID'
assert data(root,'clean','history','--older-than','1d',expected=2)['error']['code']=='PINSET_ARGUMENT_INVALID'
outside = Path('/opt') / ('pinset-contract-'+str(os.getpid()));outside.mkdir()
try:
    assert data(outside,'check',expected=1)['error']['code'] == 'PINSET_SELECTION_MISSING'
finally: outside.rmdir()

# Inject interrupted transactions to exercise the public repair command without downloading SDKs.
def interrupt(context, identity):
    import tomllib
    before=(context/'config.toml').read_text()
    project_id=tomllib.loads(before)['project_id']
    changed=before.replace('[policy]', '[policy]\nminimum_release_age = "1d"')
    version=data(root,'self','info')['version']
    lock=f'protocol = "pinset/3"\nschema = 3\nproject_id = "{project_id}"\ngenerated_by = "{version}"\ntool = []\n'
    journal={'protocol':'pinset/3','id':identity,'project_id':project_id,
             'root':str(context if context.name=='global' else context.parent), 'phase':'prepared',
             'old_config':before,'old_lock':None,'new_config':changed,'new_lock':lock,'profile_before':{}}
    directory=home/'v3/state/transactions';directory.mkdir(parents=True,exist_ok=True)
    (directory/(identity+'.json')).write_text(json.dumps(journal))
    (context/'local').mkdir(exist_ok=True)
    (context/'local/transaction.json').write_text(json.dumps({'protocol':'pinset/3','id':identity}))
    (context/'config.toml').write_text(changed);(context/'lock.toml').write_text(lock)
    return before
repair=project('repair')
repair_config=repair/'.pinset/config.toml'
import re
repair_config.write_text(re.sub(r'project_id = "[^"]+"', 'project_id = "self-update"', repair_config.read_text()))
project_before=interrupt(repair/'.pinset','repair-project')
global_context=home/'v3/global';global_context.mkdir(exist_ok=True)
(global_context/'config.toml').write_text('protocol = "pinset/3"\nschema = 3\nproject_id = "global-repair"\n[policy]\n')
global_before=interrupt(global_context,'repair-global')
repair_paths=[repair,home]
snapshot=lambda:[(str(p),hashlib.sha256(p.read_bytes()).hexdigest()) for base in repair_paths for p in sorted(base.rglob('*')) if p.is_file()]
before=snapshot()
assert data(repair,'self','repair','--plan')['transactions']==['repair-project','repair-global']
assert snapshot()==before, 'repair preview wrote state'
assert data(repair,'check',expected=1)['error']['code']=='PINSET_TRANSACTION_PENDING'
assert data(repair,'self','repair')['transactions']==['repair-project','repair-global']
for context,original in [(repair/'.pinset',project_before),(global_context,global_before)]:
    assert (context/'config.toml').read_text()==original
    assert not (context/'lock.toml').exists() and not (context/'local/transaction.json').exists()
assert data(repair,'self','repair')['transactions']==[]
assert data(repair,'check')['report']['checks']==[]
interrupt(global_context,'repair-first-global')
first_journal=home/'v3/state/transactions/repair-first-global.json'
first=json.loads(first_journal.read_text());first['old_config']='';first_journal.write_text(json.dumps(first))
assert data(repair,'self','repair')['transactions']==['repair-first-global']
assert not (global_context/'config.toml').exists()
assert subprocess.run([BIN,'-C',root,'env','access','request','--ci'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode != 0
report('contracts', public_commands=commands)
