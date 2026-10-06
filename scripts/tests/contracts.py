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
assert commands == ['init','use','remove','install','list','which','check','exec','upgrade','env','clean','self'], commands
for obsolete in ['task','workspace','doctor','setup','providers','bundle','migrate','shim','__env-resolve']:
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
assert check['report']['checks'] == [] and not check['user_command_verified']
data(root,'install','--plan');data(root,'remove','java','--plan');data(root,'clean','cache','--plan')
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
outside = Path('/opt') / ('pinset-contract-'+str(os.getpid()));outside.mkdir()
try:
    assert data(outside,'check',expected=1)['error']['code'] == 'PINSET_SELECTION_MISSING'
finally: outside.rmdir()
assert subprocess.run([BIN,'-C',root,'env','access','request','--ci'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode != 0
report('contracts', public_commands=commands)
