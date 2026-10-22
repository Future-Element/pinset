"""Compile the current Windows x86_64 target without cross-architecture execution."""
import os

from support import report, run

target = 'x86_64-pc-windows-gnu'
run(['rustup', 'target', 'add', target], timeout=600)
environment = dict(os.environ, CC_x86_64_pc_windows_gnu='x86_64-w64-mingw32-gcc')
run(['cargo', 'check', '--workspace', '--all-targets', '--locked', '--target', target],
    env=environment, timeout=1800)
report('windows-target', status='passed', target=target,
       native_runtime='not run; target compilation and platform contracts only',
       arm64='not run at user request', macos='not run at user request')
