"""Release metadata validation only. No test, probe, scan or compilation runs here."""
import hashlib,json,os,re,subprocess
from pathlib import Path
from verification_policy import LARGE_ARTIFACT_BOUNDARY, RUNTIME_EXEMPTIONS

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

def validate(report,root,tag):
    if report.get('protocol')!='pinset-verification/3' or report.get('suite')!='all':raise ValueError('a v3 local all report is required')
    if report.get('execution')!='local-docker':raise ValueError('report was not produced by local Docker')
    if not report.get('source_clean'):raise ValueError('release source contained uncommitted changes')
    commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
    if report.get('commit')!=commit:raise ValueError('tag and report commits differ')
    if report.get('source_fingerprint')!=source_fingerprint(root):raise ValueError('source fingerprint differs')
    if report.get('lock_sha256')!=hashlib.sha256((Path(root)/'Cargo.lock').read_bytes()).hexdigest():raise ValueError('lock digest differs')
    if not re.fullmatch(r'v3\.\d+\.\d+(?:-rc\.\d+)?',tag):raise ValueError('tag must name a Pinset 3 release')
    import tomllib
    versions=[tomllib.loads((Path(root)/'Cargo.toml').read_text())['workspace']['package']['version'],json.loads((Path(root)/'editors/vscode/package.json').read_text())['version']]
    if any(version!=tag[1:] for version in versions):raise ValueError('tag, crate and extension versions differ')
    if any(report.get('suites',{}).get(s,{}).get('status')!='passed' for s in ['fast','acceptance','platform','integrations']):raise ValueError('required local suite did not pass')
    if any(v.get('status')!='passed' for v in report['suites'].values()) or not report.get('commands') or any(c.get('exit_code')!=0 for c in report['commands']):raise ValueError('report includes failed or missing execution evidence')
    if not report.get('image_digests') or any(not re.fullmatch(r'sha256:[a-f0-9]{64}',d) for d in report['image_digests'].values()):raise ValueError('image digests are missing')
    evidence=report.get('evidence_files',{})
    for filename in ['contracts.json','distribution.json','cargo-security.json','real-sdks.json','platform.json','integrations.json','editor-host.json','flutter-contracts.json','frozen-selectors.json']:
        item=evidence.get(filename,{})
        if not re.fullmatch(r'[a-f0-9]{64}',item.get('sha256','')) or item.get('bytes',0)<=0:raise ValueError('required evidence digest is missing: '+filename)
    if 'QEMU' not in report.get('arm64_execution',''):raise ValueError('ARM64 emulation must be recorded')
    boundaries=report.get('unverified',[])
    if not any('Windows' in v for v in boundaries) or not any('macOS' in v for v in boundaries):raise ValueError('native platform limits must be recorded')
    if report.get('runtime_exemptions')!=RUNTIME_EXEMPTIONS or LARGE_ARTIFACT_BOUNDARY not in boundaries:raise ValueError('authorized large-artifact exemption and unverified scope must be recorded')
    return commit

if __name__=='__main__':
    payload=os.environ['PINSET_LOCAL_REPORT'].encode()
    if hashlib.sha256(payload).hexdigest()!=os.environ['PINSET_LOCAL_REPORT_SHA256']:raise ValueError('local report digest differs')
    report=json.loads(payload)
    print(validate(report,Path.cwd(),os.environ['PINSET_RELEASE_TAG']))
