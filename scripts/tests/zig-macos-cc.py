#!/usr/bin/env python3
"""Use Zig's bundled Darwin headers for target compilation, never native acceptance."""
import os,sys
args=[];skip=False
for arg in sys.argv[1:]:
    if skip:skip=False;continue
    if arg=='-arch':skip=True;continue
    if arg.startswith(('--target=','-target=')):continue
    args.append(arg)
os.execv(os.environ['PINSET_ZIG'],[os.environ['PINSET_ZIG'],'cc','-target','aarch64-macos']+args)
