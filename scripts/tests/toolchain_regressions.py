"""Partial installation, dependency closure and policy enforcement; no SDK download."""
import base64, json, os, re, shutil, tempfile, tomllib
from pathlib import Path
from support import data, native, project, report


def tool_block(root, name):
    text=(Path(root)/'.pinset/lock.toml').read_text()
    tools=tomllib.loads(text)['tool']
    for block, tool in zip(re.split(r'(?m)^\[\[tool\]\]\s*\n',text)[1:],tools):
        if tool['name']==name:return '[[tool]]\n'+block, tool
    raise AssertionError('missing fixture tool: '+name)


def write_selection(root, selections, policy=''):
    identifier=tomllib.loads((root/'.pinset/config.toml').read_text())['project_id']
    (root/'.pinset/config.toml').write_text('protocol="pinset/3"\nschema=3\nproject_id='+json.dumps(identifier)+'\n[tools]\n'+
        ''.join(name+'='+json.dumps(tool['requested'])+'\n' for name,_,tool in selections)+policy)
    (root/'.pinset/lock.toml').write_text('protocol="pinset/3"\nschema=3\nproject_id='+json.dumps(identifier)+'\ngenerated_by="3.0.5"\n'+''.join(block for _,block,_ in selections))


def cache_entry(artifact):
    integrity=artifact.get('integrity') or 'sha256:'+artifact['sha256']
    if integrity.startswith('sha512-'):return Path('sha512')/(base64.b64decode(integrity[7:]).hex()+'.archive')
    algorithm, digest=integrity.split(':',1)
    return Path(algorithm)/(digest+'.archive')


def verify_toolchain_regressions(runtime, python_project):
    node_block, node=tool_block(runtime,'node')
    pnpm_block, pnpm=tool_block(runtime,'pnpm')
    python_block, python=tool_block(python_project,'python')
    go_block, go=tool_block(runtime,'go')
    shared_home=Path(os.environ['PINSET_HOME'])/'v3'
    records=[]
    with tempfile.TemporaryDirectory(prefix='pinset-partial-install-') as temporary:
        root=Path(temporary); env=dict(os.environ,PINSET_HOME=str(root/'home'),PINSET_NO_ENV='1',
            HTTPS_PROXY='http://127.0.0.1:1',HTTP_PROXY='http://127.0.0.1:1',https_proxy='http://127.0.0.1:1',http_proxy='http://127.0.0.1:1',NO_PROXY='',no_proxy='')
        for key in ['PINSET_IDENTITY','PINSET_PROFILE','PINSET_ENV_RESOLVED']:env.pop(key,None)
        project_root=root/'project';project_root.mkdir()
        data(project_root,'init',env=env)
        for tool in [node,pnpm]:
            artifact=next(a for a in tool['artifact'] if a['target']=='linux-x86_64')
            for selected in [artifact,*artifact.get('overlay',[])]:
                relative=cache_entry(selected); source=shared_home/'cache/downloads'/relative
                assert source.is_file(),source
                target=root/'home/v3/cache/downloads'/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,target)
        flutter=dict(python,name='flutter',requested='3.35.4',version='3.35.4',provider='flutter-official')
        flutter['artifact']=[dict(python['artifact'][0],canonical_url='https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_3.35.4-stable.tar.xz',sha256='0'*64)]
        flutter['artifact'][0].pop('integrity',None)
        flutter_block='[[tool]]\nname="flutter"\nrequested="3.35.4"\nversion="3.35.4"\nprovider="flutter-official"\n[[tool.artifact]]\ntarget="linux-x86_64"\ncanonical_url='+json.dumps(flutter['artifact'][0]['canonical_url'])+'\nartifact_path="fixture.tar.xz"\nsha256="'+('0'*64)+'"\nformat="tar.xz"\narchive_root="flutter"\nverification="flutter-release-json-sha256"\n'
        selections=[('node',node_block,node),('python',python_block,python),('flutter',flutter_block,flutter)]
        write_selection(project_root,selections)
        data(project_root,'install','node','--offline',env=env)
        assert not (project_root/'.venv').exists(), 'Node install created an unrelated Python environment'
        assert native(project_root,'node','--version',env=env).strip()=='v'+node['version']
        checked=data(project_root,'check',env=env)['report']['checks']
        assert next(c for c in checked if c['tool']=='node')['bound']
        for name in ['python','flutter']:
            item=next(c for c in checked if c['tool']==name)
            assert not item['installed'] and not item['bound'] and 'PINSET_INSTALL_MISSING' in item['detail'],item
        data(project_root,'install','node','--recreate-venv',expected=1,env=env)
        node_sdk=Path(data(project_root,'which','node',env=env)['sdk'])
        assert node_sdk.resolve().is_relative_to((root/'home').resolve());shutil.rmtree(node_sdk)
        selections.append(('pnpm',pnpm_block,pnpm));write_selection(project_root,selections)
        plan=data(project_root,'install','pnpm','--offline','--plan',env=env)
        assert set(plan['tools'])=={'node','pnpm'},plan
        data(project_root,'install','pnpm','--offline',env=env)
        assert native(project_root,'pnpm','--version',env=env).strip()==pnpm['version']
        assert not (project_root/'.venv').exists()
        assert not (root/'home/v3/installs/flutter').exists(), 'partial install touched exempt Flutter SDK'
        for project_id in ['shims','registry','maintenance','self-update']:
            for filename in ['config.toml','lock.toml']:
                path=project_root/'.pinset'/filename
                path.write_text(re.sub(r'(?m)^project_id\s*=.*$', 'project_id='+json.dumps(project_id), path.read_text()))
            data(project_root,'install','node','--offline',env=env,timeout=30)
            assert native(project_root,'node','--version',env=env).strip()=='v'+node['version']
        records.append({'partial_install':'Node independent of missing Python/Flutter','pnpm_dependency':'locked Node installed','network':'disabled; verified cache only'})
        records.append({'project_lock_names':'internal resource names do not deadlock project installation'})

    policies=[('signed-checksum','\n[policy]\nminimum_verification="signed-checksum"\n','PINSET_POLICY_VIOLATION'),
        ('invalid-age','\n[policy]\nminimum_release_age="bogus"\n','PINSET_POLICY_INVALID'),
        ('too-new','\n[policy]\nminimum_release_age="9999d"\n','PINSET_RELEASE_TOO_NEW' if go.get('released-at') else 'PINSET_RELEASE_AGE_UNAVAILABLE')]
    policy_root=project('policy-regression')
    write_selection(policy_root,[('go',go_block,go)])
    assert native(policy_root,'go','version').strip().startswith('go version go'+go['version'])
    for name, policy, code in policies:
        write_selection(policy_root,[('go',go_block,go)],policy)
        for args in [('which','go'),('check',),('install','--offline')]:
            result=data(policy_root,*args,expected=1)
            assert result['error']['code']==code,result
        native(policy_root,'go','version',expected=1)
        records.append({'policy':name,'failure':code})
    report('toolchain-regressions',results=records)
    return records
