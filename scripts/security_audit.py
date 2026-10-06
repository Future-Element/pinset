"""Run the full advisory scan; narrowly document an unreachable private-RSA advisory."""
import datetime, json, os, subprocess
from pathlib import Path

process=subprocess.run(['cargo','audit','--deny','warnings','--json'],stdout=subprocess.PIPE,text=True)
report=json.loads(process.stdout)
accepted=[];failures=[]
uses=[p for p in Path('crates').rglob('*.rs') if 'pgp::' in p.read_text() or 'rsa::' in p.read_text()]
public_only=uses==[Path('crates/pinset-engine/src/node_trust.rs')]
text=Path('crates/pinset-engine/src/node_trust.rs').read_text()
public_only=public_only and not any(token in text for token in ['SignedSecretKey','SecretKeyParams','RsaPrivateKey','.decrypt('])
for finding in report.get('vulnerabilities',{}).get('list',[]):
    if (finding['advisory']['id']=='RUSTSEC-2023-0071' and finding['package']['name']=='rsa'
        and finding['package']['version']=='0.9.10' and public_only
        and datetime.date.today()<=datetime.date(2026,12,31)):
        accepted.append({'advisory':'RUSTSEC-2023-0071','package':'rsa 0.9.10',
          'scope':'pgp verifies Node public-key signatures only; no private RSA keys or private RSA operations',
          'expires':'2026-12-31','source':'https://rustsec.org/advisories/RUSTSEC-2023-0071.html'})
    else: failures.append(finding)
if any(report.get('warnings',{}).values()):failures.append(report['warnings'])
report['applicability_exceptions']=accepted
directory=Path(os.environ.get('PINSET_ACCEPTANCE_REPORTS','/tmp/pinset-reports'));directory.mkdir(parents=True,exist_ok=True)
(directory/'cargo-security.json').write_text(json.dumps(report,indent=2))
print(json.dumps({'exceptions':accepted,'unaccepted_findings':len(failures)}))
if process.returncode and not accepted and not failures:raise SystemExit(process.returncode)
raise SystemExit(1 if failures else 0)
