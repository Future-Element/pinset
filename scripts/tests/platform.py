"""Cross target compilation and actual ARM64 SDK runs in local QEMU."""
import json,os,shutil,tarfile,tempfile
from pathlib import Path
from support import *

TARGETS=['aarch64-unknown-linux-gnu','x86_64-pc-windows-gnu','aarch64-apple-darwin']
run(['rustup','target','add',*TARGETS],timeout=600)
zig_archive=download('https://ziglang.org/download/0.15.2/zig-x86_64-linux-0.15.2.tar.xz',
  '02aa270f183da276e5b5920b1dac44a63f1a49e55050ebde3aecc9eb82f93239','zig-0.15.2.tar.xz')
zig=CACHE/'zig-x86_64-linux-0.15.2'
if not zig.exists():
    with tarfile.open(zig_archive) as archive:
        for entry in archive.getmembers():
            path=(CACHE/entry.name).resolve();path.relative_to(CACHE.resolve())
            if entry.issym():(path.parent/entry.linkname).resolve().relative_to(CACHE.resolve())
            if entry.islnk():(CACHE/entry.linkname).resolve().relative_to(CACHE.resolve())
            if not(entry.isfile() or entry.isdir() or entry.issym() or entry.islnk()):raise ValueError('unsafe cross-compiler archive')
        archive.extractall(CACHE)
wrapper=Path(tempfile.mkdtemp(prefix='pinset-cross-'))/'zig-macos-cc'
wrapper.write_text(Path('scripts/tests/zig-macos-cc.py').read_text());wrapper.chmod(0o755)
cross=os.environ.copy();cross.update({'PINSET_ZIG':str(zig/'zig'),'CC_aarch64_apple_darwin':str(wrapper),
 'AWS_LC_SYS_CMAKE_BUILDER':'0','CC_aarch64_unknown_linux_gnu':'aarch64-linux-gnu-gcc',
 'CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER':'aarch64-linux-gnu-gcc',
 'CC_x86_64_pc_windows_gnu':'x86_64-w64-mingw32-gcc'})
for target in TARGETS:run(['cargo','check','--workspace','--all-targets','--locked','--target',target],env=cross,timeout=1800)
run(['cargo','build','--workspace','--bins','--locked','--target',TARGETS[0]],env=cross,timeout=1800)
arm_dir=Path(tempfile.mkdtemp(prefix='pinset-arm-bin-'))
for name in ['pinset','pinset-shim']:shutil.copy2(Path(os.environ['CARGO_TARGET_DIR'])/TARGETS[0]/'debug'/name,arm_dir/name)
arm=arm_dir/'pinset'
qemu=['qemu-aarch64-static','-cpu','cortex-a72','-L','/']
os.environ['QEMU_CPU']='cortex-a72'
assert 'pinset 3.0.0' in run(qemu+[arm,'--version'])
root=Path(tempfile.mkdtemp(prefix='pinset-arm-project-'))
def arm_cli(*args,expected=0,timeout=1200):
    return json.loads(run(qemu+[arm,'-C',root,'--json',*args],cwd=root,expected=expected,timeout=timeout))
arm_cli('init')
manifest=[]
for feature in ['8','11','17','21','25','latest']:
    arm_cli('use',frozen_selector('java@'+feature),'--no-install')
    lock=__import__('tomllib').loads((root/'.pinset/lock.toml').read_text())['tool'][0]
    assert lock['artifact'][0]['target']=='linux-aarch64'
    arm_cli('install',timeout=1800)
    route=arm_cli('which','java','--explain');sdk=Path(route['sdk'])
    assert (sdk/'include/jni.h').is_file() and (sdk/'release').is_file()
    freeze_selector('java@'+feature,lock['version'])
    # The emulator launches the exact locked SDK; QEMU is explicit external tooling.
    version=run(qemu+[sdk/'bin/java','-version'],timeout=120)
    assert 'Temurin' in version,version
    (root/'Main.java').write_text('public class Main { public static void main(String[] a) { System.out.println("PINSET_ARM_OK"); }}')
    run(qemu+[sdk/'bin/javac',str(root/'Main.java')],timeout=300)
    assert 'PINSET_ARM_OK' in run(qemu+[sdk/'bin/java','-cp',root,'Main'],timeout=120)
    routed=run(qemu+[arm,'-C',root,'exec','--no-env','--','java','-cp',root,'Main'],cwd=root,timeout=180)
    assert 'PINSET_ARM_OK' in routed
    assert arm_cli('check','--probe')['report']['checks'][0]['actually_verified']
    manifest.append(lock)
# The release matrix also verifies every other official ARM64 runtime that exists.
config=root/'.pinset/config.toml'
text=config.read_text()
if 'profile =' not in text:text=text.replace('[rust]','[rust]\nprofile = "minimal"')
text=text.replace('profile = "default"','profile = "minimal"')
config.write_text(text)
arm_sdks=[]
for spec,command,args,token in [
 ('node@lts','node',['-e','console.log("ARM_NODE_OK")'],'ARM_NODE_OK'),
 ('pnpm@10','pnpm',['--version'],None),
 ('bun@latest','bun',['-e','console.log("ARM_BUN_OK")'],'ARM_BUN_OK'),
 ('go@latest','go',['version'],'go version'),
 ('python@3.12','python',['-c','import sys; print("ARM_PYTHON_OK",sys.prefix)'],'ARM_PYTHON_OK'),
 ('rust@stable','rustc',['--version'],'rustc')]:
    arm_cli('use',frozen_selector(spec),'--no-install')
    lock=__import__('tomllib').loads((root/'.pinset/lock.toml').read_text())
    selected=next(tool for tool in lock['tool'] if tool['name']==spec.split('@')[0])
    arm_cli('install',selected['name'],timeout=3600)
    output=run(qemu+[arm,'-C',root,'exec','--no-env','--',command,*args],cwd=root,timeout=300)
    if token:assert token in output,output
    if selected['name']=='rust':
        sdk=Path(arm_cli('which','rustc')['sdk']);assert not (sdk/'bin/rustfmt').exists() and not (sdk/'bin/clippy-driver').exists()
    arm_sdks.append(selected)
# Flutter has no official Linux ARM64 archive. It must produce an explicit failure.
missing=arm_cli('use',frozen_selector('flutter@3.35.4'),'--no-install',expected=1)
assert missing['error']['code'] in ['PINSET_PLATFORM_UNAVAILABLE','PINSET_FLUTTER_TARGET_UNAVAILABLE'],missing
report('platform',status='passed',targets=TARGETS,java_arm64=manifest,other_arm64=arm_sdks,flutter_arm64=missing['error'],
  arm64_execution='QEMU user emulation; exact locked Temurin java and javac',
  windows='target compilation and platform contracts only; native runtime unverified',
  macos='target compilation using Zig Darwin headers and platform contracts only; native runtime unverified')
