#!/usr/bin/env python3
"""Package the compiled native application without modifying the iPad project."""
from pathlib import Path
import argparse, platform, plistlib, re, shutil, subprocess
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output-dir',type=Path,default=ROOT/'dist',help='Package separately without replacing a running app')
parser.add_argument('--dmg',action='store_true',help='macOS: also build a drag-to-Applications disk image')
args=parser.parse_args()
# Release file names and the app's version both come from Cargo.toml.
version=re.search(r'^version\s*=\s*"([^"]+)"',(ROOT/'Cargo.toml').read_text(),re.M).group(1)
arch={'amd64':'x64','x86_64':'x64','aarch64':'arm64','arm64':'arm64'}.get(platform.machine().lower(),platform.machine().lower())
subprocess.run(['cargo','build','--release','--locked'],check=True,cwd=ROOT)
dist=args.output_dir.resolve();dist.mkdir(parents=True,exist_ok=True)
if platform.system()=='Darwin':
    # Start from an empty bundle so files from earlier builds never linger.
    app=dist/'Pig Pal.app';shutil.rmtree(app,ignore_errors=True)
    mac=app/'Contents/MacOS';mac.mkdir(parents=True,exist_ok=True)
    resources=app/'Contents/Resources';resources.mkdir(parents=True,exist_ok=True)
    for old_notice in mac.glob('*LICENSE.txt'):
        shutil.move(str(old_notice),str(resources/old_notice.name))
    shutil.copy2(ROOT/'target/release/pigpal',mac/'pigpal')
    for path in (ROOT/'bin').iterdir():
        # Resource bundles (e.g. FluidAudio's) are data, not code: they belong in
        # Resources. In MacOS, codesign rejects them when they lack an Info.plist.
        if path.is_dir():shutil.copytree(path,resources/path.name,dirs_exist_ok=True)
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
          'CFBundleExecutable':'pigpal','CFBundleVersion':version,'CFBundleShortVersionString':version,
          'CFBundlePackageType':'APPL','LSMinimumSystemVersion':'15.0','NSHighResolutionCapable':True,
          'NSMicrophoneUsageDescription':'Record audio only when you explicitly start the microphone in a session.',
          'NSCameraUsageDescription':'Record the video input you explicitly select for a session.'}
    (app/'Contents/Info.plist').write_bytes(plistlib.dumps(info))
    # Ad-hoc sign each helper executable, then the app itself (inside out, without --deep).
    for helper in sorted(mac.iterdir()):
        if helper.is_file() and helper.name!='pigpal':
            subprocess.run(['codesign','--force','--sign','-',str(helper)],check=True)
    subprocess.run(['codesign','--force','--sign','-',str(app)],check=True)
    subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
    print(app)
    if args.dmg:
        # A plain disk image holding the app beside an Applications shortcut.
        staging=dist/'dmg-staging'
        shutil.rmtree(staging,ignore_errors=True);staging.mkdir()
        shutil.copytree(app,staging/app.name,symlinks=True)
        (staging/'Applications').symlink_to('/Applications')
        dmg=dist/f'PigPal-{version}-macos-{arch}.dmg';dmg.unlink(missing_ok=True)
        subprocess.run(['hdiutil','create','-volname','Pig Pal','-srcfolder',str(staging),
                        '-fs','HFS+','-format','UDZO','-ov',str(dmg)],check=True)
        shutil.rmtree(staging)
        print(dmg)
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
    shutil.make_archive(str(dist/f'PigPal-{version}-{platform.system().lower()}-{arch}'),'zip',dest)
    print(dest)
