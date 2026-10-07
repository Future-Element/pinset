"""Real official SDK execution. Failures and missing capabilities are never skipped."""
import json, os, re, select, shutil, subprocess, time, zipfile
from pathlib import Path
from support import *

manifest = []
def select_tool(root, spec):
    data(root,'use',frozen_selector(spec),'--no-install',timeout=1200)
    import tomllib
    locked = tomllib.loads((root/'.pinset/lock.toml').read_text())
    manifest.append({'requested':spec,'project':str(root),'lock':locked})
    (REPORTS/'sdk-manifest.json').write_text(json.dumps(manifest,indent=2))
    data(root,'install',timeout=3600)
    tool=next(item for item in locked['tool'] if item['name']==spec.split('@',1)[0])
    freeze_selector(spec,tool['version'])
    return tool

def debugger(root):
    args=[str(BIN),'-C',str(root),'exec','--no-env','--','jdb','Main']
    child=subprocess.Popen(args,cwd=root,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    transcript=b''
    def wait_for(token):
        nonlocal transcript
        deadline=time.monotonic()+30
        while token not in transcript:
            if time.monotonic()>deadline: raise AssertionError('jdb timeout: '+transcript.decode(errors='replace'))
            if select.select([child.stdout],[],[],0.2)[0]:
                block=os.read(child.stdout.fileno(),4096)
                if not block: raise AssertionError('jdb ended: '+transcript.decode(errors='replace'))
                transcript+=block
    try:
        wait_for(b'Initializing jdb')
        child.stdin.write(b'stop in Main.main\n');child.stdin.flush();wait_for(b'Deferring breakpoint')
        child.stdin.write(b'run\n');child.stdin.flush();wait_for(b'Breakpoint hit:')
        child.stdin.write(b'where\n');child.stdin.flush();wait_for(b'Main.main (Main.java:')
        child.stdin.write(b'cont\nexit\n');child.stdin.flush();child.communicate(timeout=30)
    finally:
        if child.poll() is None: child.kill();child.wait()
    (REPORTS/('jdb-'+root.name+'.log')).write_bytes(transcript)

def java(feature):
    root=project('java-'+feature)
    lock=select_tool(root,'java@'+feature)
    route=data(root,'which','java','--explain');sdk=Path(route['sdk'])
    major=int(lock['version'].split('.')[0])
    probe=data(root,'check','--deep','--probe')
    assert probe['report']['checks'][0]['actually_verified'],probe
    (root/'Main.java').write_text('public class Main { public static void main(String[] args) { System.out.println("PINSET_JAVA_OK"); System.out.println(System.getProperty("java.home")); }}\n')
    (root/'classes').mkdir();native(root,'javac','-d','classes','Main.java')
    assert 'PINSET_JAVA_OK' in native(root,'java','-cp','classes','Main')
    assert str(sdk) in native(root,'java','-cp','classes','Main')
    native(root,'jar','cfe','hello.jar','Main','-C','classes','.')
    assert 'PINSET_JAVA_OK' in native(root,'java','-jar','hello.jar')
    native(root,'javadoc','-d','docs','Main.java')
    assert (root/'docs/Main.html').exists()
    native(root,'javap','-verbose','-classpath','classes','Main')
    native(root,'keytool','-genkeypair','-alias','test','-keyalg','RSA','-keystore','test.p12','-storepass','temporary123','-keypass','temporary123','-dname','CN=PinsetTest','-validity','1')
    native(root,'jarsigner','-keystore','test.p12','-storepass','temporary123','hello.jar','test')
    assert 'jar verified' in native(root,'jarsigner','-verify','hello.jar')
    shutil.copy(root/'classes/Main.class',root/'Main.class');debugger(root);(root/'Main.class').unlink()
    if major==8:
        assert data(root,'which','jmod',expected=1)['error']['code']=='PINSET_COMMAND_UNAVAILABLE'
        assert Path(data(root,'which','javah')['executable']).is_file()
    if major>=11: assert data(root,'which','javah',expected=1)['error']['code']=='PINSET_COMMAND_UNAVAILABLE'
    if major>=9:
        assert 'PINSET_SHELL_OK' in native(root,'jshell','--feedback','silent',stdin='System.out.println("PINSET_SHELL_OK");\n/exit\n')
        assert 'java.base' in native(root,'jdeps','hello.jar')
        mod=root/'mod';mod.mkdir();(mod/'module-info.java').write_text('module pinset.example { exports demo; }')
        (mod/'demo').mkdir();(mod/'demo/Main.java').write_text('package demo; public class Main { public static void main(String[] a) { System.out.println("PINSET_MODULE_OK"); }}')
        native(root,'javac','-d','modclasses','mod/module-info.java','mod/demo/Main.java')
        (root/'test-jmods').mkdir();native(root,'jmod','create','--class-path','modclasses','test-jmods/example.jmod')
        module_path=str(root/'test-jmods')
        if (sdk/'jmods').is_dir():module_path=str(sdk/'jmods')+':'+module_path
        else:assert major>=24 and 'Linking from run-time image enabled' in native(root,'jlink','--help')
        native(root,'jlink','--module-path',module_path,'--add-modules','pinset.example','--output','image')
        assert 'PINSET_MODULE_OK' in run([root/'image/bin/java','-m','pinset.example/demo.Main'],cwd=root)
    if major>=11:
        assert 'PINSET_JAVA_OK' in native(root,'java','Main.java')
        native(root,'javac','--release','8','-d','release8','Main.java')
        assert 'major version: 52' in native(root,'javap','-verbose','-classpath','release8','Main')
    if major>=14:
        inputs=root/'package-input';inputs.mkdir();shutil.copy(root/'hello.jar',inputs/'hello.jar')
        native(root,'jpackage','--type','app-image','--name','PinsetExample','--input','package-input','--main-jar','hello.jar','--add-modules','java.base','--dest','package-output',timeout=300)
        assert 'PINSET_JAVA_OK' in run([root/'package-output/PinsetExample/bin/PinsetExample'],cwd=root)
    (root/'Sleep.java').write_text('import java.lang.management.ManagementFactory; public class Sleep { public static void main(String[] a) throws Exception { System.out.println(ManagementFactory.getRuntimeMXBean().getName().split("@")[0]); System.out.flush(); Thread.sleep(90000); }}')
    native(root,'javac','Sleep.java')
    child=subprocess.Popen([BIN,'-C',root,'exec','--no-env','--','java','Sleep'],cwd=root,stdout=subprocess.PIPE,text=True)
    try:
        pid=child.stdout.readline().strip();assert pid.isdigit(),pid
        vm=native(root,'jcmd',pid,'VM.version');assert pid+':' in vm and 'VM' in vm,vm
        native(root,'jcmd',pid,'JFR.start','name=pinset','settings=profile')
        time.sleep(1)
        native(root,'jcmd',pid,'JFR.stop','name=pinset','filename='+str(root/'test.jfr'))
        assert (root/'test.jfr').read_bytes().startswith(b'FLR\x00')
        if major>=11: native(root,'jfr','summary','test.jfr')
    finally:
        if pid.isdigit(): os.kill(int(pid),15)
        child.wait(timeout=30)
    (root/'Native.java').write_text('public class Native { static { System.loadLibrary("pinsetnative"); } private static native int answer(); public static void main(String[] a) { if(answer()!=42) throw new RuntimeException(); System.out.println("PINSET_JNI_OK"); }}')
    native(root,'javac','-h','.','Native.java')
    (root/'native.c').write_text('#include "Native.h"\nJNIEXPORT jint JNICALL Java_Native_answer(JNIEnv *env,jclass clazz) { return 42; }\n')
    native(root,'gcc','-shared','-fPIC','-I'+str(sdk/'include'),'-I'+str(sdk/'include/linux'),'native.c','-o','libpinsetnative.so')
    assert 'PINSET_JNI_OK' in native(root,'java','-Djava.library.path=.','Native')
    receipt=(sdk/'.pinset-install.toml').read_text()
    assert 'jconsole' in receipt and (sdk/'lib').is_dir()
    return root,lock,major

def python():
    a=project('python-a');b=project('python-b')
    select_tool(a,'python@3.12');select_tool(b,'python@3.13')
    for root in [a,b]:
        info=json.loads(native(root,'python','-c','import sys,site,json; print(json.dumps([sys.prefix,sys.base_prefix,site.ENABLE_USER_SITE,sys.executable]))'))
        assert info[0]==str(root/'.venv') and info[0]!=info[1] and info[2] is False
        assert str(root/'.venv') in native(root,'pip','--version')
        assert data(root,'which','pip')['prefix']==['-m','pip']
    package=a/'package';package.mkdir();(package/'setup.py').write_text('from setuptools import setup\nsetup(name="pinset-isolation-proof",version="1.0",py_modules=["pinset_isolation_proof"])')
    (package/'pinset_isolation_proof.py').write_text('VALUE=42')
    native(a,'pip','install','--no-deps',str(package),timeout=300)
    assert '42' in native(a,'python','-c','import pinset_isolation_proof; print(pinset_isolation_proof.VALUE)')
    native(b,'python','-c','import pinset_isolation_proof',expected=1)
    old=(a/'.venv/.pinset-owner.toml').read_text()
    data(a,'use','python@3.13',timeout=1200)
    assert (a/'.venv/.pinset-owner.toml').read_text()==old
    data(a,'which','python',expected=1)
    data(a,'install','--recreate-venv')
    assert list((a/'.pinset/local/venv-backups').iterdir())
    native(a,'python','-c','import pinset_isolation_proof',expected=1)
    external=project('python-external');(external/'.venv').mkdir();(external/'.venv/user-data').write_text('preserve')
    data(external,'use','python@3.13',expected=1,timeout=1200)
    data(external,'self','repair')
    assert (external/'.venv/user-data').read_text()=='preserve'
    return a,b

def runtimes():
    root=project('runtimes')
    for spec in ['node@lts','bun@latest','go@latest']:select_tool(root,spec)
    assert 'PINSET_NODE_OK' in native(root,'node','-e','console.log("PINSET_NODE_OK",process.execPath)')
    native(root,'npm','--version');native(root,'bun','--version')
    from pnpm import verify_pnpm_generations
    verify_pnpm_generations(root,select_tool)
    (root/'main.go').write_text('package main; import("fmt";"os"); func main(){fmt.Println("PINSET_GO_OK",os.Getenv("GOTOOLCHAIN"))}')
    assert 'PINSET_GO_OK local' in native(root,'go','run','main.go')
    native(root,'bun','-e','console.log("PINSET_BUN_OK")')
    rust=project('rust')
    # Resolve the real default profile, including official documentation aliases,
    # before the execution profile. Metadata-only: do not download documentation.
    data(rust,'use',frozen_selector('rust@stable'),'--no-install',timeout=1200)
    import tomllib
    default_lock=tomllib.loads((rust/'.pinset/lock.toml').read_text())
    default_tool=default_lock['tool'][0]
    assert default_tool['metadata']['profile']=='default'
    for artifact in default_tool['artifact']:
        docs=next(o for o in artifact['overlay'] if o['archive_root'].startswith('rust-docs-'))
        assert docs['canonical_url'].endswith('/'+docs['archive_root']+'.tar.xz')
    manifest.append({'requested':'rust@stable','profile':'default','metadata_only':True,'project':str(rust),'lock':default_lock})
    (REPORTS/'sdk-manifest.json').write_text(json.dumps(manifest,indent=2))
    freeze_selector('rust@stable',default_tool['version'])
    config=rust/'.pinset/config.toml'
    text=config.read_text()
    if 'profile =' not in text:text=text.replace('[rust]','[rust]\nprofile = "minimal"')
    text=text.replace('profile = "default"','profile = "minimal"').replace('components = []','components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]').replace('targets = []','targets = ["wasm32-unknown-unknown"]')
    config.write_text(text)
    # Only an explicit use may reconcile edited Rust options with the lock.
    before=(rust/'.pinset/lock.toml').read_bytes()
    assert data(rust,'install','--plan',expected=1)['error']['code']=='PINSET_LOCK_MISMATCH'
    preview=data(rust,'use',frozen_selector('rust@stable'),'--no-install','--plan',timeout=1200)
    assert preview['lock']['tool'][0]['options']['profile']=='minimal'
    assert (rust/'.pinset/lock.toml').read_bytes()==before
    select_tool(rust,'rust@stable')
    (rust/'main.rs').write_text('fn main(){println!("PINSET_RUST_OK");}')
    native(rust,'rustc','main.rs','-o','main');assert 'PINSET_RUST_OK' in native(rust,'./main')
    native(rust,'rustfmt','main.rs');native(rust,'rustfmt','--check','main.rs');native(rust,'rustc','--target','wasm32-unknown-unknown','--crate-type','lib','main.rs','-o','main.wasm')
    native(rust,'cargo','clippy','--version')
    native(rust,'rust-analyzer','--version')
    sdk=Path(data(rust,'which','rustc')['sdk']);assert (sdk/'lib/rustlib/src/rust/library/std/src/lib.rs').is_file()
    assert not (sdk/'share/doc/rust/html/index.html').exists()
    assert data(rust,'check','--probe')['report']['checks'][0]['actually_verified']
    return root,rust

def lifecycle():
    root=project('lifecycle');select_tool(root,'java@17')
    sdk=Path(data(root,'which','java')['sdk']);release=sdk/'release';original=release.read_bytes()
    changed=bytearray(original);changed[-1]=32 if changed[-1]!=32 else 10;release.write_bytes(changed)
    assert data(root,'check','--deep')['report']['checks'][0]['installed'] is False
    data(root,'install','--offline','--repair',timeout=600)
    assert release.read_bytes()==original
    assert data(root,'check','--deep')['report']['checks'][0]['installed']
    pending=[]
    for _ in range(2):pending.append(subprocess.Popen([BIN,'-C',root,'--json','install','--offline'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT))
    for child in pending:
        output=child.communicate(timeout=300)[0];assert child.returncode==0,output.decode()
    retained=data(root,'clean','installs','--plan')['retained']
    assert str(sdk) in retained
    data(root,'remove','java');assert release.read_bytes()==original
    data(root,'use',frozen_selector('java@17'),'--no-install',timeout=1200)
    data(root,'install','--offline',timeout=600)



def switch_versions():
    root=project('java-switch');select_tool(root,'java@17')
    (root/'verify.sh').write_text('#!/bin/sh\nset -eu\njavac Main.java\njava Main\n');(root/'verify.sh').chmod(0o755)
    (root/'Main.java').write_text('public class Main { public static void main(String[] a) { System.out.println("SWITCH_OK"); }}')
    assert 'SWITCH_OK' in native(root,'./verify.sh')
    original=data(root,'which','java')
    select_tool(root,'java@21')
    selected=data(root,'which','java')
    assert selected['version'].startswith('21.') and selected['sdk']!=original['sdk']
    assert 'SWITCH_OK' in native(root,'./verify.sh')
    assert selected['sdk'] in native(root,'sh','-c','printf "%s" "$JAVA_HOME"')
    data(root,'use','java@'+original['version'],timeout=1200)
    assert data(root,'which','java')['sdk']==original['sdk']
    assert 'SWITCH_OK' in native(root,'./verify.sh')
    (root/'Main.java').write_text('invalid source')
    native(root,'./verify.sh',expected=1)
    assert data(root,'which','java')['version']==original['version']
    assert not (Path(os.environ['PINSET_HOME'])/'v3/state/candidates').exists()
    assert not (Path(os.environ['PINSET_HOME'])/'v3/state/history').exists()
    return root

if __name__=='__main__':
    roots=[]
    try:
        for feature in ['8','11','17','21','25','latest']:roots.append(java(feature))
        python();runtimes();switch_versions();lifecycle()
        report('real-sdks',status='passed',manifest=manifest,java_versions=[lock['version'] for _,lock,_ in roots],
               gui_scope='JDK inventory only here; actual GUI starts belong to integrations')
    except Exception:
        report('real-sdks',manifest=manifest,status='failed');raise
