"""One no-activation window verifies page/service lifecycle through the shared CLI."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import uuid
import sys

ROOT=Path(__file__).resolve().parents[2]
CLI=ROOT/'target/debug/lazybuddy-cli.exe'
HOST=ROOT/'target/debug/lazybuddy.exe'
from windows_fixture import user32,children


before=user32.GetForegroundWindow()
capture_dir=Path(sys.argv[sys.argv.index('--capture-dir')+1]).resolve() if '--capture-dir' in sys.argv else None
if capture_dir:capture_dir.mkdir(parents=True,exist_ok=False)
with tempfile.TemporaryDirectory(prefix='qa-host-lifecycle-',dir=ROOT/'target') as temporary:
    directory=Path(temporary)
    environment=dict(os.environ,LAZYBUDDY_INSTANCE='qa-lifecycle-'+uuid.uuid4().hex,
                     LAZYBUDDY_DATA_DIR=str(directory/'data'))
    def cli(*args,required=True):
        result=subprocess.run([str(CLI),*args],env=environment,capture_output=True,text=True,encoding='utf-8',
            timeout=12,creationflags=subprocess.CREATE_NO_WINDOW)
        if user32.GetForegroundWindow()!=before:raise RuntimeError('foreground changed')
        if result.returncode:
            if required:raise RuntimeError('CLI failed: '+result.stderr)
            return None
        return json.loads(result.stdout)

    for identifier in ['alpha','beta']:
        package=directory/identifier
        (package/'bin').mkdir(parents=True)
        manifest=json.loads((ROOT/'contracts/fixtures/feature.json').read_text(encoding='utf-8'))
        manifest.update(id=identifier,name=identifier)
        (package/'module.json').write_text(json.dumps(manifest),encoding='utf-8')
        shutil.copyfile(ROOT/'target/debug/examples/fixture.exe',package/'bin/service.exe')
        cli('modules','install-local',str(package))
    (directory/'data').mkdir(parents=True,exist_ok=True)
    (directory/'data/preferences.json').write_text(json.dumps(dict(window=[170,180,700,500])),encoding='utf-8')
    host=subprocess.Popen([str(HOST),'--background'],env=environment,stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        def rpc(command,**args):
            reply=cli('host',json.dumps(dict(version=1,command=command,args=args)))
            if not reply['ok']:raise RuntimeError('host rejected '+command)
            return reply.get('data',reply)
        def wait(predicate):
            deadline=time.monotonic()+10
            while time.monotonic()<deadline:
                if host.poll() is not None:raise RuntimeError('host exited')
                reply=cli('host',json.dumps(dict(version=1,command='status')),required=False)
                if reply and predicate(reply['data']):return reply['data']
                time.sleep(0.05)
            raise RuntimeError('host state timeout')
        wait(lambda status:status['hidden'] and status['window']==[170,180,700,500])
        rpc('page.select',id='alpha')
        assert rpc('status')['service_active'] is False
        assert children(host.pid)==set()
        rpc('window.show')
        wait(lambda status:not status['hidden'] and not status['busy'] and status['news_sources']==2)
        first=children(host.pid)
        assert len(first)==1
        if capture_dir:rpc('window.capture',path=str(capture_dir/'news.png'))
        rpc('settings.open',category='about')
        wait(lambda status:status['section']=='Settings(About)' and not status['service_active'])
        assert children(host.pid)==set()
        if capture_dir:rpc('window.capture',path=str(capture_dir/'about.png'))
        rpc('page.select',id='beta')
        assert rpc('status')['section']=='Settings(About)'
        assert children(host.pid)==set()
        rpc('settings.back')
        wait(lambda status:status['root']=='beta' and not status['busy'] and status['news_sources']==2)
        second=children(host.pid)
        assert len(second)==1 and not first.intersection(second)
        rpc('page.select',id='alpha')
        wait(lambda status:status['root']=='alpha' and not status['busy'] and status['service_active'])
        third=children(host.pid)
        assert len(third)==1 and not second.intersection(third)
        normal=rpc('status')['window']
        rpc('window.dock',edge='left')
        compact=wait(lambda status:status['capsule'] and not status['service_active'])
        assert children(host.pid)==set()
        assert compact['window'][2]<150 and compact['window'][3]<200,compact['window']
        if capture_dir:rpc('window.capture',path=str(capture_dir/'capsule.png'))
        rpc('window.expand')
        expanded=wait(lambda status:not status['capsule'] and status['service_active'] and not status['busy'])
        assert expanded['window']==normal,(normal,expanded['window'])
        rpc('window.hide')
        wait(lambda status:status['hidden'] and not status['service_active'])
        assert children(host.pid)==set()
        rpc('app.quit')
        host.wait(timeout=10)
        assert host.returncode==0
        print(json.dumps(dict(page_switch=True,reactivation=True,global_settings_preserved=True,capsule_pauses_and_restores=True,hidden_has_no_services=True,
            owned_services_stopped=True,foreground_unchanged=True,upstream_requests=False,visual_checked=False)))
    finally:
        if host.poll() is None:host.terminate();host.wait(timeout=5)
