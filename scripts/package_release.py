"""Packaging and artifact inventory. Deliberately contains no validation commands."""
import hashlib,json,os,subprocess,zipfile
from pathlib import Path
root=Path.cwd();target=os.environ['PINSET_RELEASE_TARGET'];version=os.environ['PINSET_RELEASE_VERSION'].removeprefix('v');destination=root/'dist';destination.mkdir(exist_ok=True)
suffix='.exe' if target.startswith('windows-') else ''
archive=destination/f'pinset-v{version}-{target}.zip'
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as output:
    for name in ['pinset','pinset-shim']:
        path=root/'target/release'/(name+suffix)
        if not path.is_file() or path.stat().st_size==0:raise RuntimeError('compiled release artifact is missing')
        info=zipfile.ZipInfo(name+suffix);info.external_attr=(0o100755<<16);info.compress_type=zipfile.ZIP_DEFLATED;output.writestr(info,path.read_bytes())
metadata=json.loads(subprocess.check_output(['cargo','metadata','--format-version','1','--locked'],text=True))
components=[{'type':'library','name':p['name'],'version':p['version'],'purl':f"pkg:cargo/{p['name']}@{p['version']}",'licenses':[{'expression':p['license']}] if p.get('license') else []} for p in metadata['packages']]
sbom={'bomFormat':'CycloneDX','specVersion':'1.6','version':1,'metadata':{'component':{'type':'application','name':'pinset','version':version}},'components':components}
(destination/f'pinset-{target}.sbom.json').write_text(json.dumps(sbom,sort_keys=True,separators=(',',':')))
