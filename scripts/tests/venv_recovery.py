"""Local venv interruption acceptance using a private copy of an installed SDK."""
import json, os, re, shlex, shutil, signal, subprocess, tempfile, time, tomllib
from pathlib import Path
from support import BIN, run


def verify_venv_recovery(source_project):
    source_project = Path(source_project)
    with tempfile.TemporaryDirectory(prefix='pinset-venv-recovery-') as temporary:
        fixture = Path(temporary)
        project = fixture/'project';project.mkdir()
        home = fixture/'home'
        binary = fixture/'bin';binary.mkdir()
        for name in ['pinset', 'pinset-shim']:
            shutil.copy2(BIN.parent/name, binary/name)
        env = dict(os.environ, HOME=str(fixture), PINSET_HOME=str(home), PINSET_NO_ENV='1')
        for key in ['PINSET_IDENTITY', 'PINSET_PROFILE', 'PINSET_ENV_RESOLVED']:
            env.pop(key, None)
        def cli(*arguments, expected=0, native=False):
            return run([binary/'pinset', '-C', project]+([] if native else ['--json'])+list(arguments),
                       cwd=project, env=env, expected=expected, timeout=90)
        cli('init')
        source_lock = (source_project/'.pinset/lock.toml').read_text()
        blocks = re.split(r'(?m)^\[\[tool\]\]\s*$', source_lock)
        block = next(block for block in blocks[1:] if tomllib.loads('[[tool]]\n'+block)['tool'][0]['name']=='python')
        tool = tomllib.loads('[[tool]]\n'+block)['tool'][0]
        source_id = tomllib.loads(blocks[0])['project_id']
        config = project/'.pinset/config.toml'
        project_id = tomllib.loads(config.read_text())['project_id']
        config.write_text('protocol = "pinset/3"\nschema = 3\nproject_id = '+json.dumps(project_id)+'\n[tools]\npython = '+json.dumps(tool['requested'])+'\n')
        (project/'.pinset/lock.toml').write_text(blocks[0].replace(json.dumps(source_id), json.dumps(project_id))+'[[tool]]\n'+block)
        route = json.loads(run([BIN, '-C', source_project, '--json', 'which', 'python'], timeout=30))
        source_sdk = Path(route['sdk'])
        platform = source_sdk.name
        sdk = home/'v3/installs/python'/source_sdk.parent.name/platform
        sdk.parent.mkdir(parents=True)
        # The copy has independent directories and no writes to hard-linked SDK
        # files. Fault injection atomically replaces only the private alias.
        shutil.copytree(source_sdk, sdk, copy_function=os.link, symlinks=True)
        cli('install', '--offline')
        old = project/'.venv/preserved-package';old.write_text('preserve package bytes')
        interpreter = sdk/'bin/python3'
        original = interpreter.with_name('python3-pinset-original')
        interpreter.rename(original)
        try:
            interpreter.write_text('#!/bin/sh\n'+shlex.quote(str(original))+' "$@" || exit $?\nfor destination do :; done\nmkdir "$destination/.pinset-owner.toml"\n')
            interpreter.chmod(0o755)
            cli('install', '--offline', '--recreate-venv', expected=1)
            assert old.read_text()=='preserve package bytes'
            assert not (project/'.pinset/local/venv-transaction.json').exists()
            assert not list((home/'v3/state/venv-transactions').glob('*.json'))
            active = fixture/'worker-active'
            active_stage = fixture/'worker-active.tmp'
            release = fixture/'worker-release'
            # The worker can only launch this interpreter after receiving G.
            # Keep it live until the test explicitly releases it, independent
            # of how long journal reads or recovery take on the host.
            interpreter.write_text('#!/bin/sh\nprintf "%s\\n" "$$" > '+shlex.quote(str(active_stage))+
                                   '\nmv '+shlex.quote(str(active_stage))+' '+shlex.quote(str(active))+
                                   '\nwhile [ ! -e '+shlex.quote(str(release))+' ]; do sleep 0.01; done\nexec '+
                                   shlex.quote(str(original))+' "$@"\n')
            interpreter.chmod(0o755)
            child = subprocess.Popen([binary/'pinset', '-C', project, '--json', 'install', '--offline', '--recreate-venv'],
                                     cwd=project, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            worker_pid = None
            worker_birth = None
            try:
                deadline = time.monotonic()+30
                while time.monotonic()<deadline:
                    journals = list((home/'v3/state/venv-transactions').glob('*.json'))
                    if journals:
                        journal = json.loads(journals[0].read_text())
                        if journal['worker_pid'] and journal['worker_birth']:
                            worker_pid = journal['worker_pid'];worker_birth = journal['worker_birth']
                            if active.exists():break
                    if child.poll() is not None:
                        raise AssertionError(f'venv parent exited before gated interruption: {child.returncode}')
                    time.sleep(0.01)
                assert worker_pid and active.exists(), 'venv interpreter did not start after the persisted worker handshake'
                assert not release.exists()
                active_pid = int(active.read_text())
                assert Path(f'/proc/{active_pid}/stat').read_text().rsplit(')',1)[1].split()[2]==str(worker_pid), 'active interpreter is not in the persisted worker group'
                fields = Path(f'/proc/{worker_pid}/stat').read_text().rsplit(')',1)[1].split()
                assert fields[0]!='Z' and fields[19]==worker_birth, 'persisted worker identity is no longer live'
                child.kill();child.wait(timeout=10)
                busy = json.loads(cli('self', 'repair', expected=1))
                assert busy['error']['code']=='PINSET_VENV_BUSY', busy
                assert not old.exists(), 'backup was restored while a writer was active'
                before_config = config.read_text()
                exact_lock = (project/'.pinset/lock.toml').read_text()
                config_id = 'venv-config-pending'
                transaction = home/'v3/state/transactions'/f'{config_id}.json'
                transaction.parent.mkdir(parents=True, exist_ok=True)
                changed = before_config+'\n[policy]\nminimum_release_age = "1s"\n'
                transaction.write_text(json.dumps({'protocol':'pinset/3', 'id':config_id,
                    'project_id':project_id, 'root':str(project.resolve()), 'phase':'prepared',
                    'old_config':before_config, 'old_lock':exact_lock, 'new_config':changed,
                    'new_lock':exact_lock, 'profile_before':{}}))
                config.write_text(changed)
                (project/'.pinset/local/transaction.json').write_text(json.dumps({'protocol':'pinset/3','id':config_id}))
                # The running shell retains its private wrapper inode. Restore
                # the SDK alias before validating the previous venv binding.
                interpreter.unlink();interpreter.symlink_to(original.name)
                release.touch()
                deadline = time.monotonic()+60
                while time.monotonic()<deadline:
                    stat = Path(f'/proc/{worker_pid}/stat')
                    try:fields = stat.read_text().rsplit(')',1)[1].split()
                    except FileNotFoundError:break
                    if fields[0]=='Z' or fields[19]!=worker_birth:break
                    time.sleep(0.05)
                else:raise AssertionError('interrupted venv worker did not finish')
                recovered = json.loads(cli('self', 'repair'))
                assert len(recovered['venv_transactions'])==1
                assert recovered['transactions']==[config_id]
                assert config.read_text()==before_config
                assert json.loads(transaction.read_text())['phase']=='recovered'
                assert not (project/'.pinset/local/transaction.json').exists()
                assert old.read_text()=='preserve package bytes'
                assert not (project/'.pinset/local/venv-transaction.json').exists()
                assert json.loads(cli('self', 'repair'))['venv_transactions']==[]
                child.communicate(timeout=10)
            finally:
                release.touch()
                if child.poll() is None:child.kill();child.wait(timeout=10)
                if worker_pid:
                    # This ID is the private process group we created, never a
                    # host process. Kill any fixture descendants before cleanup.
                    stat = Path(f'/proc/{worker_pid}/stat')
                    try:
                        fields = stat.read_text().rsplit(')',1)[1].split()
                    except FileNotFoundError:
                        fields = None
                    if fields:
                        if fields[0]!='Z' and fields[19]==worker_birth:
                            try:os.killpg(worker_pid, signal.SIGKILL)
                            except ProcessLookupError:pass
                child.communicate(timeout=10)
        finally:
            interpreter.unlink(missing_ok=True)
            original.rename(interpreter)
        assert 'venv-restored' in cli('exec', '--no-env', '--', 'python', '-c', 'print("venv-restored")', native=True)
        return {'owner_write_failure':'old environment restored', 'sigkill':'journal recovered after writer completion',
                'live_writer':'recovery blocked with PINSET_VENV_BUSY', 'repeat_recovery':'idempotent',
                'writer_gate':'interpreter active after G; released only after live-writer recovery rejection',
                'combined_config_and_venv_recovery':'venv restored before prior binding validation',
                'sdk_downloads':False, 'existing_sdk_modified':False}
