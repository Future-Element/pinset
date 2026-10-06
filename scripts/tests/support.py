"""Shared local-container harness. It never invokes a remote CI runner."""
import hashlib, json, os, signal, subprocess, tempfile, time, urllib.request
from pathlib import Path

BIN = Path(os.environ.get('PINSET_TEST_CLI',str(Path(os.environ.get('CARGO_TARGET_DIR', '/build/target')) / 'debug/pinset')))
CACHE = Path(os.environ.get('PINSET_ACCEPTANCE_CACHE', '/sdk-cache'))
REPORTS = Path(os.environ.get('PINSET_ACCEPTANCE_REPORTS', '/tmp/pinset-reports'))
REPORTS.mkdir(parents=True, exist_ok=True)
CASES = []

def run(args, cwd=None, expected=0, timeout=300, stdin=None, env=None):
    print('EXEC', ' '.join(map(str, args)), flush=True)
    started = time.monotonic()
    process = subprocess.Popen(list(map(str,args)),cwd=cwd,stdin=subprocess.PIPE if stdin is not None else None,
                               text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env,start_new_session=True)
    try: output,_=process.communicate(stdin,timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid,signal.SIGKILL);output,_=process.communicate()
        CASES.append({'command':list(map(str,args)),'cwd':str(cwd),'exit':124,'seconds':round(time.monotonic()-started,3)})
        raise AssertionError('acceptance command timed out: '+str(args)+'\n'+output[-12000:])
    result=subprocess.CompletedProcess(args,process.returncode,stdout=output)
    CASES.append({'command': list(map(str, args)), 'cwd': str(cwd),
                  'exit': result.returncode, 'seconds': round(time.monotonic()-started, 3)})
    if expected is not None and result.returncode != expected:
        raise AssertionError(f'{args}: exit {result.returncode}\n{result.stdout[-12000:]}')
    return result.stdout

def cli(root, *args, expected=0, native=False, **kwargs):
    return run([BIN, '-C', root] + ([] if native else ['--json']) + list(args),
               cwd=root, expected=expected, **kwargs)

def data(root, *args, **kwargs):
    return json.loads(cli(root, *args, **kwargs))

def project(name):
    root = Path(tempfile.mkdtemp(prefix='pinset-'+name+'-'))
    cli(root, 'init')
    return root

def native(root, *args, **kwargs):
    return cli(root, 'exec', '--no-env', '--', *args, native=True, **kwargs)

def download(url, sha256, name):
    CACHE.mkdir(parents=True, exist_ok=True)
    dest = CACHE / name
    if not dest.exists():
        part = dest.with_suffix('.part')
        with urllib.request.urlopen(url, timeout=120) as response, part.open('wb') as output:
            while block := response.read(1024*1024): output.write(block)
        part.replace(dest)
    actual = hashlib.sha256(dest.read_bytes()).hexdigest()
    if actual != sha256: raise AssertionError(f'{name}: checksum mismatch: {actual}')
    return dest

def official_download(url, name):
    checksum = urllib.request.urlopen(url+'.sha256', timeout=120).read().decode().split()[0]
    return download(url, checksum, name)

def report(name, **values):
    (REPORTS/(name+'.json')).write_text(json.dumps({'protocol':'pinset-acceptance/3',
      'host':'linux-x86_64', 'cases':CASES, **values}, indent=2))


def frozen_selector(spec):
    path=REPORTS/'frozen-selectors.json'
    selections=json.loads(path.read_text()) if path.exists() else {}
    return selections.get(spec,spec)

def freeze_selector(spec,version):
    path=REPORTS/'frozen-selectors.json'
    selections=json.loads(path.read_text()) if path.exists() else {}
    exact=spec.split('@',1)[0]+'@'+version
    if spec in selections and selections[spec]!=exact:raise AssertionError('SDK selection changed during verification: '+spec)
    selections[spec]=exact
    path.write_text(json.dumps(selections,sort_keys=True,indent=2))
