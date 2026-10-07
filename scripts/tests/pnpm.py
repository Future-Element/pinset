"""Real pnpm package generations and selected-Node execution; local Docker only."""
import json, os
from pathlib import Path
from support import data, native, report

def verify_pnpm_generations(root, select_tool):
    versions=[]
    node=data(root,'which','node')['executable']
    package=root/'package.json'
    npmrc=root/'.npmrc'
    workspace=root/'pnpm-workspace.yaml'
    assert not package.exists() and not npmrc.exists() and not workspace.exists()
    try:
        for selector in ['10','11','latest']:
            selected=select_tool(root,'pnpm@'+selector)
            route=data(root,'which','pnpm','--explain')
            alias=data(root,'which','pnpx','--explain')
            sdk=Path(route['sdk'])
            entry=selected.get('metadata',{}).get('pnpm-entry','bin/pnpm.cjs')
            if entry=='pnpm':
                assert route['executable']==str(sdk/'pnpm')
                assert len(selected['artifact'][0]['overlay'])==1
                assert (sdk/'dist/node_modules/node-gyp/bin/node-gyp.js').is_file()
            else:
                assert route['executable']==node and route['prefix'][0]==str(sdk/entry)
            assert alias['executable']==route['executable'] and alias['prefix'][-1]=='dlx'
            assert native(root,'pnpm','--version').strip()==selected['version']
            # Project settings cannot fetch another runtime or package manager.
            npmrc.write_text('use-node-version=0.0.0\nmanage-package-manager-versions=true\n')
            if entry!='bin/pnpm.cjs':workspace.write_text('packages:\n  - .\npmOnFail: download\nruntimeOnFail: download\n')
            package.write_text(json.dumps({'name':'pinset-pnpm-guard','version':'1.0.0','packageManager':'pnpm@0.0.0'}))
            assert '0.0.0' in native(root,'pnpm','exec','node','--version',expected=1)
            package.write_text(json.dumps({'name':'pinset-pnpm-guard','version':'1.0.0','packageManager':'pnpm@'+selected['version']}))
            assert node in native(root,'pnpm','exec','node','-e','console.log(process.execPath)')
            # Explicit installation is the only network step. With no proxy server
            # and networking disabled, both install reuse and execution still work.
            offline_env={**os.environ,'HTTPS_PROXY':'http://127.0.0.1:1','HTTP_PROXY':'http://127.0.0.1:1',
                         'https_proxy':'http://127.0.0.1:1','http_proxy':'http://127.0.0.1:1',
                         'NO_PROXY':'','no_proxy':'','COREPACK_ENABLE_NETWORK':'0'}
            assert data(root,'install','pnpm','--offline',env=offline_env)['installed']
            assert native(root,'pnpm','--version',env=offline_env).strip()==selected['version']
            assert node in native(root,'pnpm','exec','node','-e','console.log(process.execPath)',env=offline_env)
            if entry!='bin/pnpm.cjs':
                package.write_text(json.dumps({'name':'pinset-runtime-guard','version':'1.0.0','packageManager':'pnpm@'+selected['version'],
                                              'devEngines':{'runtime':{'name':'node','version':'0.0.0','onFail':'download'}}}))
                assert '0.0.0' in native(root,'pnpm','exec','node','--version',expected=1,env=offline_env)
                workspace.unlink()
            package.unlink();npmrc.unlink()
            versions.append({'selector':selector,'version':selected['version'],'entry':entry,'route':route,'pnpx':alias})
        report('pnpm-generations',versions=versions,offline_execution='no implicit binary download',node=node)
        return versions
    finally:
        for path in [package,npmrc,workspace]:
            if path.exists():path.unlink()
