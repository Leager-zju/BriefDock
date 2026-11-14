"""Explicit updater-only process handoff in a temporary install; no real accounts/network."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import uuid
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

ROOT=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--host-dir',type=Path,required=True)
parser.add_argument('--report',type=Path,required=True)
args=parser.parse_args()
binaries=args.host_dir.resolve()
def info(directory):
    files={path.relative_to(directory).as_posix():hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(directory.rglob('*')) if path.is_file() and path.name!='build-info.json'}
    build=hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
    result=dict(schema=1,version='2.0.0',build=build,files=files)
    (directory/'build-info.json').write_text(json.dumps(result),encoding='utf-8')
    return result
with tempfile.TemporaryDirectory(prefix='qa-update-process-',dir=ROOT/'target') as temporary:
    root=Path(temporary);install=root/'install';data=root/'data'
    shutil.copytree(binaries,install)
    key=Ed25519PrivateKey.generate()
    source=dict(url='https://example.invalid/update.json',public_key=list(key.public_key().public_bytes_raw()))
    (install/'app-update-source.json').write_text(json.dumps(source),encoding='utf-8')
    old_info=info(install)
    stage=root/'stage';shutil.copytree(install,stage)
    (stage/'README.txt').write_text('same version, replaced test build',encoding='utf-8')
    new_info=info(stage)
    assert old_info['version']==new_info['version'] and old_info['build']!=new_info['build']
    directory=root/'updates'/new_info['build'];directory.mkdir(parents=True)
    shutil.move(str(stage),str(directory/'stage'))
    manifest=dict(schema=1,target='windows-x86_64',version='2.0.0',build=new_info['build'],url='https://example.invalid/app.zip',bytes=1,sha256='0'*64,host_sha256=new_info['files']['lazybuddy.exe'])
    payload=json.dumps(manifest).encode()
    (directory/'manifest.json').write_bytes(payload);(directory/'manifest.json.sig').write_bytes(key.sign(payload))
    data.mkdir();(data/'modules').mkdir();(data/'modules/keep').write_text('module data',encoding='utf-8')
    (data/'preferences.json').write_text(json.dumps(dict(auto_update=False,dark=True)),encoding='utf-8')
    env=dict(os.environ,LAZYBUDDY_INSTANCE='qa-update-'+uuid.uuid4().hex,LAZYBUDDY_DATA_DIR=str(data))
    old=subprocess.Popen([str(install/'lazybuddy.exe'),'--background'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
    helper=None
    def rpc(command,required=True):
        result=subprocess.run([str(binaries/'lazybuddy-cli.exe'),'host',json.dumps(dict(version=1,command=command,args={}))],env=env,capture_output=True,encoding='utf-8',timeout=10,creationflags=subprocess.CREATE_NO_WINDOW)
        if result.returncode:
            if required:raise RuntimeError('host control failed')
            return None
        reply=json.loads(result.stdout)
        if not reply.get('ok'):
            if required:raise RuntimeError('host rejected request')
            return None
        return reply.get('data',reply)
    def wait(predicate,timeout=40):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            state=rpc('status',required=False)
            if state and predicate(state):return state
            time.sleep(.1)
        raise RuntimeError('updater process state timeout')
    try:
        wait(lambda state:state['pid']==old.pid and state['window'] is not None)
        (directory/'job.json').write_text(json.dumps(dict(install=str(install),parent_pid=old.pid)),encoding='utf-8')
        shutil.copyfile(binaries/'lazybuddy-updater.exe',directory/'apply.exe')
        helper=subprocess.Popen([str(directory/'apply.exe'),'--apply',str(directory/'job.json')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
        deadline=time.monotonic()+12
        while not (directory/'ready').exists():
            assert helper.poll() is None, 'helper exited before handoff'
            assert time.monotonic()<deadline, 'helper handoff timeout'
            time.sleep(.05)
        assert old.poll() is None
        assert json.loads((install/'build-info.json').read_text(encoding='utf-8'))['build']==old_info['build']
        rpc('app.quit');old.wait(timeout=20)
        helper.wait(timeout=45);assert helper.returncode==0, 'update helper failed'
        state=wait(lambda state:state['pid']!=old.pid and not state['hidden'])
        assert json.loads((install/'build-info.json').read_text(encoding='utf-8'))['build']==new_info['build']
        assert (data/'modules/keep').read_text(encoding='utf-8')=='module data'
        preferences=json.loads((data/'preferences.json').read_text(encoding='utf-8'))
        assert preferences['dark'] is True and preferences['auto_update'] is False
        assert not (directory/'stage').exists() and not (directory/'backup').exists()
        report=dict(passed=True,waited_for_normal_exit=True,same_version_build_changed=True,restarted_pid_changed=True,user_data_preserved=True,staging_cleaned=True,real_account_tested=False,network_used=False)
    finally:
        if rpc('status',required=False):rpc('app.quit')
        if old.poll() is None:old.wait(timeout=20)
        if helper and helper.poll() is None:
            try:helper.wait(timeout=35)
            except subprocess.TimeoutExpired:helper.kill();helper.wait()
        if rpc('status',required=False):rpc('app.quit',required=False)
        deadline=time.monotonic()+10
        while rpc('status',required=False) and time.monotonic()<deadline:time.sleep(.1)

args.report.parent.mkdir(parents=True,exist_ok=True)
args.report.write_text(json.dumps(report,indent=2),encoding="utf-8")
print(json.dumps(report))
