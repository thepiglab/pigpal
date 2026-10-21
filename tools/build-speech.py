#!/usr/bin/env python3
"""Build the platform speech adapter. Python is a build tool only."""
from pathlib import Path
import argparse, os, platform, shutil, subprocess
ROOT = Path(__file__).resolve().parents[1]
REV = '5c9b0fbbd8e611d88e47d7e1ebae9bca9b447e8f'
def run(*args, **kw): subprocess.run(args, check=True, cwd=ROOT, **kw)
def copy_file(source, destination):
    # Prior builds may preserve a dependency license's read-only mode.
    if destination.exists(): destination.chmod(destination.stat().st_mode | 0o200)
    shutil.copy2(source, destination)
parser=argparse.ArgumentParser()
parser.add_argument('--portable', action='store_true')
parser.add_argument('--cmake', default='cmake')
args=parser.parse_args()
out=ROOT/'bin'; out.mkdir(exist_ok=True)
if platform.system()=='Darwin' and not args.portable:
    run('swift','build','-c','release','--package-path',str(ROOT/'native/macos'))
    binary=ROOT/'native/macos/.build/release/pigpal-speech-macos'
    copy_file(binary,out/binary.name)
    for bundle in binary.parent.glob('*.bundle'):
        shutil.copytree(bundle,out/bundle.name,dirs_exist_ok=True)
    copy_file(ROOT/'native/macos/.build/checkouts/FluidAudio/LICENSE',out/'FluidAudio-LICENSE.txt')
else:
    repo=ROOT/'vendor/parakeet.cpp'
    if not repo.exists(): run('git','clone','https://github.com/mudler/parakeet.cpp.git',str(repo))
    run('git','-C',str(repo),'checkout',REV)
    run('git','-C',str(repo),'submodule','update','--init','--recursive')
    build=repo/'build-desktop'
    run(args.cmake,'-S',str(repo),'-B',str(build),'-DCMAKE_BUILD_TYPE=Release',
        '-DBUILD_SHARED_LIBS=OFF','-DGGML_NATIVE=OFF','-DGGML_METAL=OFF',
        '-DGGML_BLAS=OFF','-DPARAKEET_BUILD_SERVER=OFF','-DPARAKEET_WITH_CED=OFF',
        '-DPARAKEET_WITH_VOICEDETECT=OFF')
    run(args.cmake,'--build',str(build),'--config','Release','--target','parakeet-cli','--parallel',str(min(os.cpu_count() or 2,8)))
    name='parakeet-cli.exe' if platform.system()=='Windows' else 'parakeet-cli'
    matches=[p for p in build.rglob(name) if p.is_file()]
    if len(matches)!=1: raise SystemExit(f'Expected one speech binary, found {matches}')
    copy_file(matches[0],out/name)
    copy_file(repo/'LICENSE',out/'parakeet-LICENSE.txt')
    copy_file(repo/'third_party/ggml/LICENSE',out/'ggml-LICENSE.txt')
print(f'Speech adapter ready in {out}')
