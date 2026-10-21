#!/usr/bin/env python3
"""Package the compiled native application without modifying the iPad project."""
from pathlib import Path
import argparse, platform, plistlib, shutil, subprocess
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output-dir',type=Path,default=ROOT/'dist',help='Package separately without replacing a running app')
args=parser.parse_args()
subprocess.run(['cargo','build','--release','--locked'],check=True,cwd=ROOT)
dist=args.output_dir.resolve();dist.mkdir(parents=True,exist_ok=True)
if platform.system()=='Darwin':
    app=dist/'Pig Pal.app';mac=app/'Contents/MacOS';mac.mkdir(parents=True,exist_ok=True)
    resources=app/'Contents/Resources';resources.mkdir(parents=True,exist_ok=True)
    for old_notice in mac.glob('*LICENSE.txt'):
        shutil.move(str(old_notice),str(resources/old_notice.name))
    shutil.copy2(ROOT/'target/release/pigpal',mac/'pigpal')
    for path in (ROOT/'bin').iterdir():
        if path.is_dir():shutil.copytree(path,mac/path.name,dirs_exist_ok=True)
        else:
            destination=(resources if path.suffix=='.txt' else mac)/path.name
            if destination.exists():destination.chmod(0o755 if path.suffix!='.txt' else 0o644)
            shutil.copy2(path,destination)
            if path.suffix=='.txt':destination.chmod(0o644)
    for name in ['README.md','THIRD_PARTY.md']:shutil.copy2(ROOT/name,resources/name)
    shutil.copytree(ROOT/'docs',resources/'docs',dirs_exist_ok=True)
    shutil.copy2(ROOT/'assets/fonts/LICENSE.txt',resources/'Inter-LICENSE.txt')
    shutil.copy2(ROOT/'third_party/cpal/LICENSE',resources/'CPAL-LICENSE.txt')
    info={'CFBundleName':'Pig Pal','CFBundleDisplayName':'Pig Pal','CFBundleIdentifier':'org.research.pigpal.desktop',
          'CFBundleExecutable':'pigpal','CFBundleVersion':'1','CFBundleShortVersionString':'0.1.0',
          'CFBundlePackageType':'APPL','LSMinimumSystemVersion':'15.0','NSHighResolutionCapable':True,
          'NSMicrophoneUsageDescription':'Record audio only when you explicitly start the microphone in a session.',
          'NSCameraUsageDescription':'Record the video input you explicitly select for a session.'}
    (app/'Contents/Info.plist').write_bytes(plistlib.dumps(info))
    subprocess.run(['codesign','--force','--deep','--sign','-',str(app)],check=True)
    print(app)
else:
    dest=dist/'PigPal';dest.mkdir(exist_ok=True)
    ext='.exe' if platform.system()=='Windows' else ''
    shutil.copy2(ROOT/f'target/release/pigpal{ext}',dest/f'PigPal{ext}')
    for binary in (ROOT/'bin').iterdir():
        if binary.is_file():shutil.copy2(binary,dest/binary.name)
    for name in ['README.md','THIRD_PARTY.md']:shutil.copy2(ROOT/name,dest/name)
    shutil.copytree(ROOT/'docs',dest/'docs',dirs_exist_ok=True)
    shutil.copy2(ROOT/'assets/fonts/LICENSE.txt',dest/'Inter-LICENSE.txt')
    shutil.copy2(ROOT/'third_party/cpal/LICENSE',dest/'CPAL-LICENSE.txt')
    shutil.make_archive(str(dist/f'PigPal-{platform.system()}-{platform.machine()}'),'zip',dest)
    print(dest)
