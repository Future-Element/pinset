"""Real pnpm package generations and selected-Node execution; local Docker only."""
import json, os, shutil
from pathlib import Path
from support import data, native, report, run

def verify_pnpm_generations(root, select_tool):
    versions=[]
    node=data(root,'which','node')['executable']
    package=root/'package.json'
    npmrc=root/'.npmrc'
    workspace=root/'pnpm-workspace.yaml'
    assert not package.exists() and not npmrc.exists() and not workspace.exists()
    try:
        for selector in ['10.34.7','11','latest']:
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
            # Version/exec probes do not validate install-specific launch options.
            # Install one local dependency offline and run its module with the
            # selected Node, through both exec and the public pnpm shim.
            dependency=root/'local-dependency';dependency.mkdir()
            (dependency/'package.json').write_text(json.dumps({'name':'pinset-pnpm-proof','version':'1.0.0','main':'index.js'}))
            (dependency/'index.js').write_text('module.exports = 42\n')
            application={'name':'pinset-pnpm-install','version':'1.0.0','packageManager':'pnpm@'+selected['version'],
                         'dependencies':{'pinset-pnpm-proof':'file:./local-dependency'}}
            package.write_text(json.dumps(application))
            if entry=='bin/pnpm.cjs':
                # Reproduce the old prefix against the actual pnpm parser.
                rejected=run([node,sdk/entry,'--use-node-version=','install','--offline','--ignore-scripts'],
                             cwd=root,env=offline_env,expected=1)
                assert 'Unknown option' in rejected and 'use-node-version' in rejected,rejected
            native(root,'pnpm','install','--offline','--ignore-scripts',env=offline_env)
            observed=native(root,'pnpm','exec','node','-e','console.log(require("pinset-pnpm-proof"),process.execPath)',env=offline_env)
            assert '42' in observed and node in observed,observed
            shim=Path(os.environ['PINSET_HOME'])/'v3/bin/pnpm'
            run([shim,'install','--offline','--ignore-scripts'],cwd=root,env={**offline_env,'PINSET_NO_ENV':'1'})
            shutil.rmtree(root/'node_modules');(root/'pnpm-lock.yaml').unlink();shutil.rmtree(dependency)
            if entry!='bin/pnpm.cjs':
                package.write_text(json.dumps({'name':'pinset-runtime-guard','version':'1.0.0','packageManager':'pnpm@'+selected['version'],
                                              'devEngines':{'runtime':{'name':'node','version':'0.0.0','onFail':'download'}}}))
                assert '0.0.0' in native(root,'pnpm','exec','node','--version',expected=1,env=offline_env)
                workspace.unlink()
            package.unlink();npmrc.unlink()
            versions.append({'selector':selector,'version':selected['version'],'entry':entry,'route':route,'pnpx':alias})
        report('pnpm-generations',versions=versions,offline_execution='no implicit binary download',
               offline_install='local dependency install and selected Node execution via exec and shim',node=node)
        return versions
    finally:
        for path in [package,npmrc,workspace]:
            if path.exists():path.unlink()
