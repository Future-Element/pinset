"""Release metadata validation only. No test, probe, scan or compilation runs here."""
import hashlib,json,os,re,subprocess
from pathlib import Path
import urllib.request
from verification_policy import (LARGE_ARTIFACT_BOUNDARY, RUNTIME_EXEMPTIONS,
    HOTFIX_SCOPE, HOTFIX_COMMANDS, HOTFIX_EVIDENCE, HOTFIX_UNVERIFIED,
    X64_SCOPE, X64_ARM64_EXECUTION, X64_SUITES, X64_COMMANDS, X64_EVIDENCE, X64_UNVERIFIED)

# Explicitly authorized for this release only; ordinary verification is unchanged.
EXCEPTION_PROTOCOL = 'pinset-release-exception/1'
EXCEPTION_TAG = 'v3.1.1'
EXCEPTION_REPOSITORY = 'Future-Element/pinset'
EXCEPTION_IMPLEMENTATION_SHA = 'a4e131e6673cd88e673d83f914bd2ce2ff3ac8e3'
EXCEPTION_CI_RUN_ID = 40873625789
EXCEPTION_CI_WORKFLOW = '.github/workflows/verify-platforms.yml'
EXCEPTION_ALLOWED_PATHS = frozenset({
    'scripts/release_gate.py', 'scripts/tests/release_exception.py',
    'scripts/tests/surfaces.py', '.github/workflows/release.yml',
    'docs/windows-release-validation.md', 'CHANGELOG.md', 'CONTRIBUTING.md',
})
EXCEPTION_WORKFLOW_REPLACEMENTS = (
    ('description: Existing v3 tag matching the clean local all, x64 or authorized hotfix report',
     'description: Existing v3 tag matching the local report or authorized 3.1.1 exception'),
    ('description: Exact compact JSON from the matching local Docker verification report',
     'description: Exact compact local Docker report JSON or the explicit 3.1.1 exception record'),
    ('name: Generate release checksums and preserve local verification evidence',
     'name: Generate release checksums and preserve verification or exception evidence'),
)
EXCEPTION_ARTIFACTS = {
    'native-source-linux-x86_64': {'id': 12897745928, 'size_in_bytes': 9789,
        'digest': 'sha256:73aac4aa14a72de115a9d9347fbb3f545a2ecf656c9cb45027788c32b79eb946'},
    'native-source-linux-aarch64': {'id': 12896683683, 'size_in_bytes': 9800,
        'digest': 'sha256:c6471c3f00f64b122169568a19ee49881d0e8891ad500572eabe80ac0933d0c1'},
    'native-source-macos-aarch64': {'id': 12896947357, 'size_in_bytes': 9838,
        'digest': 'sha256:041374f0f5d274233b2d194ce5a8c012d7bda816cc53d857fcd34aa69eb21fd6'},
    'native-source-windows-x86_64': {'id': 12896699258, 'size_in_bytes': 10944,
        'digest': 'sha256:d30848b878b8ff1668afa0b289644b8dbd9a22ed67204ddfd3ca25ec5c35fa65'},
    'windows-source-candidate': {'id': 12896639596, 'size_in_bytes': 7693060,
        'digest': 'sha256:3c0ee977b5a4c789879fc61ce88dd9f7c9862413c1a1916cf06ae3a656755496'},
}
EXCEPTION_WINDOWS_CANDIDATE = {
    'archive_sha256': 'e0a977eb07dd90f54911013954ec99f7f711f244f796a62d25d11377c54ea331',
    'binaries': {
        'pinset.exe': 'ba4bff4c60ed215dbf66943788a90c61fdc7b0562d374e3589e1981c0879fe1d',
        'pinset-shim.exe': '135d8cb4a4b9ebfdcd416346b491c44a0e5afb099f0eff95f943ad376ff61b55',
    },
    'import_inspection': 'passed', 'hosted_native_smoke': 'passed',
}
EXCEPTION_AUTHORIZATION = 'User approved the v3.1.1-only Docker exception on 2026-10-29T12:59:07Z'
EXCEPTION_DOCKER = {'status': 'skipped',
    'reason': 'User explicitly requested direct v3.1.1 publication without installing Docker; no passing local Docker report exists'}
EXCEPTION_UNVERIFIED = [
    'Local Docker full-suite verification skipped by explicit user authorization',
    'Clean Windows x64 without Visual C++ Redistributable and WinGet installation/uninstallation not verified',
    'Linux/macOS native CI smoke used debug binaries; final release archives require separate verification',
    'Dependency security audits and full SDK/editor/website/credential-store integrations not rerun; Dependabot #22 remains open',
    'Defender concurrency error 0x8050111c remains outside this CRT repair',
    LARGE_ARTIFACT_BOUNDARY,
]


def public_github_metadata(endpoint):
    """Anonymous public metadata only; permission/rate/network errors fail closed."""
    base = f'repos/{EXCEPTION_REPOSITORY}/actions/runs/{EXCEPTION_CI_RUN_ID}'
    if endpoint not in {base, base + '/jobs?per_page=100', base + '/artifacts?per_page=100'}:
        raise ValueError('exception metadata endpoint is outside the pinned CI run')
    request = urllib.request.Request('https://api.github.com/' + endpoint, headers={
        'Accept': 'application/vnd.github+json', 'User-Agent': 'pinset-release-gate',
        'X-GitHub-Api-Version': '2022-11-28',
    })
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            if response.status != 200 or response.geturl() != request.full_url:
                raise ValueError('public CI metadata response changed endpoint/status')
            data = response.read(2 * 1024 * 1024 + 1)
            if len(data) > 2 * 1024 * 1024:
                raise ValueError('public CI metadata exceeds size limit')
            return json.loads(data)
    except (OSError, ValueError) as error:
        raise ValueError('public CI metadata could not be verified: ' + str(error)) from error


def validate_release_exception(report, root, tag, query=None):
    """Validate the authorized 3.1.1 exception, never an all-suite pass."""
    required = {'protocol', 'tag', 'authorization', 'implementation_commit', 'commit',
                'source_clean', 'source_fingerprint', 'lock_sha256', 'local_docker',
                'ci', 'windows_candidate', 'unverified'}
    if set(report) != required or report.get('protocol') != EXCEPTION_PROTOCOL:
        raise ValueError('invalid one-release exception schema; no all-suite claim is accepted')
    if tag != EXCEPTION_TAG or report['tag'] != EXCEPTION_TAG:
        raise ValueError('the release exception applies only to v3.1.1')
    if report['authorization'] != EXCEPTION_AUTHORIZATION or report['local_docker'] != EXCEPTION_DOCKER:
        raise ValueError('explicit authorization and skipped Docker verification must be recorded')
    if report['implementation_commit'] != EXCEPTION_IMPLEMENTATION_SHA:
        raise ValueError('exception implementation commit differs from the tested source')
    if report['unverified'] != EXCEPTION_UNVERIFIED:
        raise ValueError('exception unverified boundaries must be preserved')
    if report['windows_candidate'] != EXCEPTION_WINDOWS_CANDIDATE:
        raise ValueError('exception Windows candidate hashes or checks differ')
    expected_ci = {'repository': EXCEPTION_REPOSITORY, 'workflow': EXCEPTION_CI_WORKFLOW,
                   'run_id': EXCEPTION_CI_RUN_ID, 'run_attempt': 1,
                   'head_sha': EXCEPTION_IMPLEMENTATION_SHA, 'artifacts': EXCEPTION_ARTIFACTS}
    if report['ci'] != expected_ci:
        raise ValueError('exception CI identity or artifact digests differ')
    root = Path(root)
    def git(*arguments):
        return subprocess.check_output(['git', *arguments], cwd=root, stderr=subprocess.PIPE)
    commit = git('rev-parse', 'HEAD').decode().strip()
    if report['source_clean'] is not True or report['commit'] != commit:
        raise ValueError('exception report must match the clean final release commit')
    status = git('status', '--porcelain', '--untracked-files=all', '-z').split(b'\0')
    if any(not line[3:].startswith(b'output/') for line in status if line):
        raise ValueError('exception release source contains uncommitted changes')
    if report['source_fingerprint'] != source_fingerprint(root):
        raise ValueError('exception source fingerprint differs')
    if report['lock_sha256'] != hashlib.sha256((root / 'Cargo.lock').read_bytes()).hexdigest():
        raise ValueError('exception Cargo lock digest differs')
    try:
        git('merge-base', '--is-ancestor', EXCEPTION_IMPLEMENTATION_SHA, 'HEAD')
        changed = git('diff', '--no-renames', '--name-only', '-z', EXCEPTION_IMPLEMENTATION_SHA, 'HEAD')
    except subprocess.CalledProcessError as error:
        raise ValueError('tested implementation is not an ancestor of the release') from error
    paths = {name.decode() for name in changed.split(b'\0') if name}
    if not paths <= EXCEPTION_ALLOWED_PATHS:
        raise ValueError('untested business/build/dependency changes after native CI: ' + ', '.join(sorted(paths - EXCEPTION_ALLOWED_PATHS)))
    workflow = '.github/workflows/release.yml'
    baseline = git('show', EXCEPTION_IMPLEMENTATION_SHA + ':' + workflow)
    approved = baseline
    for before, after in EXCEPTION_WORKFLOW_REPLACEMENTS:
        if approved.count(before.encode()) != 1:
            raise ValueError('release workflow exception baseline differs')
        approved = approved.replace(before.encode(), after.encode(), 1)
    if (root / workflow).read_bytes() not in (baseline, approved):
        raise ValueError('release workflow changed beyond the three approved description updates')
    import tomllib
    versions = [tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']['version'],
                json.loads((root / 'editors/vscode/package.json').read_text())['version'],
                json.loads((root / 'website/package.json').read_text())['version'],
                json.loads((root / 'release.json').read_text())['published']]
    if any(version != EXCEPTION_TAG[1:] for version in versions):
        raise ValueError('exception release component versions differ')
    query = query or public_github_metadata
    endpoint = f'repos/{EXCEPTION_REPOSITORY}/actions/runs/{EXCEPTION_CI_RUN_ID}'
    run = query(endpoint)
    expected_run = {'id': EXCEPTION_CI_RUN_ID, 'head_sha': EXCEPTION_IMPLEMENTATION_SHA,
                    'path': EXCEPTION_CI_WORKFLOW, 'event': 'workflow_dispatch',
                    'run_attempt': 1, 'status': 'completed', 'conclusion': 'success'}
    if (not isinstance(run, dict) or any(run.get(key) != value for key, value in expected_run.items())
            or not isinstance(run.get('repository'), dict) or not isinstance(run.get('head_repository'), dict)
            or run.get('repository', {}).get('full_name') != EXCEPTION_REPOSITORY
            or run.get('head_repository', {}).get('full_name') != EXCEPTION_REPOSITORY):
        raise ValueError('pinned native CI run identity/status could not be verified')
    jobs = query(endpoint + '/jobs?per_page=100')
    expected_jobs = {f'Native {target} (source)' for target in
                     ('linux-x86_64', 'linux-aarch64', 'windows-x86_64', 'macos-aarch64')}
    items = jobs.get('jobs', []) if isinstance(jobs, dict) else []
    if (not isinstance(jobs, dict) or jobs.get('total_count') != 4 or not isinstance(items, list) or len(items) != 4
            or any(not isinstance(item, dict) for item in items)
            or {item.get('name') for item in items} != expected_jobs
            or any(item.get('head_sha') != EXCEPTION_IMPLEMENTATION_SHA
                   or item.get('status') != 'completed' or item.get('conclusion') != 'success' for item in items)):
        raise ValueError('all four pinned native CI jobs must have completed successfully')
    artifacts = query(endpoint + '/artifacts?per_page=100')
    items = artifacts.get('artifacts', []) if isinstance(artifacts, dict) else []
    if (not isinstance(artifacts, dict) or artifacts.get('total_count') != len(EXCEPTION_ARTIFACTS)
            or not isinstance(items, list) or len(items) != len(EXCEPTION_ARTIFACTS)
            or any(not isinstance(item, dict) for item in items)
            or {item.get('name') for item in items} != set(EXCEPTION_ARTIFACTS)):
        raise ValueError('pinned CI artifact inventory is incomplete or changed')
    for item in items:
        expected = EXCEPTION_ARTIFACTS[item['name']]
        if (any(item.get(key) != value for key, value in expected.items())
                or item.get('expired') is not False
                or not isinstance(item.get('workflow_run'), dict)
                or item.get('workflow_run', {}).get('id') != EXCEPTION_CI_RUN_ID
                or item.get('workflow_run', {}).get('head_sha') != EXCEPTION_IMPLEMENTATION_SHA):
            raise ValueError('pinned native CI artifact identity/digest is invalid: ' + item['name'])
    return commit

def source_fingerprint(root):
    result=subprocess.check_output(['git','-c',f'safe.directory={root}','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root)
    files=[]
    for raw in result.split(b'\0'):
        if not raw:continue
        name=raw.decode('utf-8')
        if name.startswith(('output/','target/','plan/','.pinset/','website/.next/','website/out/','website/node_modules/','editors/vscode/dist/','editors/vscode/node_modules/')):continue
        path=Path(root)/name
        if path.is_file():files.append((name,path))
    files=sorted(set(files))
    paths=b''.join(name.encode()+b'\0' for name,_ in files)
    raw=subprocess.check_output(['git','-c',f'safe.directory={root}','check-attr','-z','--stdin','text'],input=paths,cwd=root)
    fields=raw.split(b'\0')
    attributes={fields[i].decode():fields[i+2] for i in range(0,len(fields)-1,3)}
    digest=hashlib.sha256()
    for name,path in files:
        encoded=name.encode();data=path.read_bytes();text=attributes.get(name,b'unspecified')
        # Honor Git's explicit binary boundary. Auto text is normalized only
        # for UTF-8 without NUL; binary bytes always remain part of the digest.
        if text==b'set':data=data.replace(b'\r\n',b'\n')
        elif text==b'auto' and b'\0' not in data:
            try:data.decode('utf-8')
            except UnicodeDecodeError:pass
            else:data=data.replace(b'\r\n',b'\n')
        digest.update(len(encoded).to_bytes(8,'little'));digest.update(encoded);digest.update(len(data).to_bytes(8,'little'));digest.update(data)
    return digest.hexdigest()

def validate(report,root,tag,query=None):
    if report.get('protocol')==EXCEPTION_PROTOCOL:return validate_release_exception(report,root,tag,query)
    if report.get('protocol')!='pinset-verification/3' or report.get('suite') not in ['all','hotfix','x64']:raise ValueError('a v3 local all, authorized x64 or hotfix report is required')
    if report.get('execution')!='local-docker':raise ValueError('report was not produced by local Docker')
    if not report.get('source_clean'):raise ValueError('release source contained uncommitted changes')
    commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
    if report.get('commit')!=commit:raise ValueError('tag and report commits differ')
    if report.get('source_fingerprint')!=source_fingerprint(root):raise ValueError('source fingerprint differs')
    if report.get('lock_sha256')!=hashlib.sha256((Path(root)/'Cargo.lock').read_bytes()).hexdigest():raise ValueError('lock digest differs')
    if not re.fullmatch(r'v3\.\d+\.\d+(?:-rc\.\d+)?',tag):raise ValueError('tag must name a Pinset 3 release')
    import tomllib
    versions=[tomllib.loads((Path(root)/'Cargo.toml').read_text())['workspace']['package']['version'],json.loads((Path(root)/'editors/vscode/package.json').read_text())['version'],json.loads((Path(root)/'website/package.json').read_text())['version'],json.loads((Path(root)/'release.json').read_text())['published']]
    if any(version!=tag[1:] for version in versions):raise ValueError('tag, crate, extension, website and release versions differ')
    hotfix=report['suite']=='hotfix'
    x64=report['suite']=='x64'
    if hotfix:
        if report.get('hotfix_scope')!=HOTFIX_SCOPE:raise ValueError('hotfix scope differs from the authorized checks')
        if set(report.get('suites',{}))!={'hotfix'} or report['suites']['hotfix'].get('status')!='passed':raise ValueError('required hotfix checks did not pass')
        commands=report.get('commands',[])
        if [(c.get('label'),c.get('argv')) for c in commands]!=HOTFIX_COMMANDS or any(c.get('cwd')!='/workspace' for c in commands):raise ValueError('hotfix execution commands differ from the authorized checks')
        if report.get('unverified')!=HOTFIX_UNVERIFIED+[LARGE_ARTIFACT_BOUNDARY] or report.get('arm64_execution')!='Not run for this hotfix':raise ValueError('hotfix unverified scope must be explicit')
    elif x64:
        if report.get('x64_scope')!=X64_SCOPE:raise ValueError('x64 scope differs from the authorized checks')
        if set(report.get('suites',{}))!=set(X64_SUITES) or any(report['suites'][name].get('status')!='passed' for name in X64_SUITES):raise ValueError('required x64 checks did not pass')
        commands=report.get('commands',[])
        if [(c.get('label'),c.get('argv')) for c in commands]!=X64_COMMANDS or any(c.get('cwd')!='/workspace' for c in commands):raise ValueError('x64 execution commands differ from the authorized checks')
        if report.get('unverified')!=X64_UNVERIFIED+[LARGE_ARTIFACT_BOUNDARY] or report.get('arm64_execution')!=X64_ARM64_EXECUTION:raise ValueError('x64 unverified scope must be explicit')
    elif any(report.get('suites',{}).get(s,{}).get('status')!='passed' for s in ['fast','acceptance','platform','integrations']):raise ValueError('required local suite did not pass')
    if any(v.get('status')!='passed' for v in report['suites'].values()) or not report.get('commands') or any(c.get('exit_code')!=0 for c in report['commands']):raise ValueError('report includes failed or missing execution evidence')
    if not report.get('image_digests') or any(not re.fullmatch(r'sha256:[a-f0-9]{64}',d) for d in report['image_digests'].values()):raise ValueError('image digests are missing')
    evidence=report.get('evidence_files',{})
    required=HOTFIX_EVIDENCE if hotfix else X64_EVIDENCE if x64 else ['contracts.json','distribution.json','cargo-security.json','real-sdks.json','platform.json','integrations.json','editor-host.json','flutter-contracts.json','frozen-selectors.json']
    for filename in required:
        item=evidence.get(filename,{})
        if not re.fullmatch(r'[a-f0-9]{64}',item.get('sha256','')) or item.get('bytes',0)<=0:raise ValueError('required evidence digest is missing: '+filename)
    if not hotfix and not x64 and 'QEMU' not in report.get('arm64_execution',''):raise ValueError('ARM64 emulation must be recorded')
    boundaries=report.get('unverified',[])
    if not any('Windows' in v for v in boundaries) or not any('macOS' in v for v in boundaries):raise ValueError('native platform limits must be recorded')
    if report.get('runtime_exemptions')!=RUNTIME_EXEMPTIONS or LARGE_ARTIFACT_BOUNDARY not in boundaries:raise ValueError('authorized large-artifact exemption and unverified scope must be recorded')
    return commit

if __name__=='__main__':
    payload=os.environ['PINSET_LOCAL_REPORT'].encode()
    if hashlib.sha256(payload).hexdigest()!=os.environ['PINSET_LOCAL_REPORT_SHA256']:raise ValueError('local report digest differs')
    report=json.loads(payload)
    print(validate(report,Path.cwd(),os.environ['PINSET_RELEASE_TAG']))
