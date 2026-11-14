"""Offline shared-shell navigation, rectangular region and wide/narrow capture."""
import ctypes
import sys
from ctypes import wintypes
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import uuid
from windows_fixture import user32, children

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT/'target/verification-ui'
OUTPUT.mkdir(exist_ok=True)
DARK = '--dark' in sys.argv
with tempfile.TemporaryDirectory(prefix='qa-shell-', dir=ROOT/'target') as temporary:
    directory = Path(temporary)
    binaries = directory/'bin'
    binaries.mkdir()
    for name in ['lazybuddy.exe', 'lazybuddy-cli.exe']:
        shutil.copyfile(ROOT/'target/debug'/name, binaries/name)
    # Deliberate offline catalog failure also checks the bottom information bar.
    (binaries/'module-catalog.json').write_text(json.dumps(dict(url='https://127.0.0.1:1/catalog.json', public_key=[0]*32)), encoding='utf-8')
    env = dict(os.environ, LAZYBUDDY_INSTANCE='qa-shell-'+uuid.uuid4().hex, LAZYBUDDY_DATA_DIR=str(directory/'data'), NO_PROXY='127.0.0.1')
    def cli(*args, required=True):
        result = subprocess.run([str(binaries/'lazybuddy-cli.exe'), *args], env=env, capture_output=True, encoding='utf-8', timeout=15, creationflags=subprocess.CREATE_NO_WINDOW)
        if result.returncode:
            if required: raise RuntimeError(result.stderr)
            return None
        return json.loads(result.stdout)
    for identifier, name in [('news','新闻速览'), ('hongguo','红果短剧')]:
        package=directory/identifier
        (package/'bin').mkdir(parents=True)
        manifest=json.loads((ROOT/'contracts/fixtures/feature.json').read_text(encoding='utf-8'))
        manifest.update(id=identifier,name=name,arguments=['--many-sources'])
        (package/'module.json').write_text(json.dumps(manifest),encoding='utf-8')
        shutil.copyfile(ROOT/'target/debug/examples/fixture.exe',package/'bin/service.exe')
        cli('modules','install-local',str(package))
    (directory/'data/preferences.json').write_text(json.dumps(dict(window=[170,180,780,560],dark=DARK)),encoding='utf-8')
    host=subprocess.Popen([str(binaries/'lazybuddy.exe'),'--background'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
    def rpc(command, **args):
        reply=cli('host',json.dumps(dict(version=1,command=command,args=args)))
        assert reply['ok'],reply
        return reply.get('data',reply)
    def wait(predicate,timeout=20):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            reply=cli('host',json.dumps(dict(version=1,command='status',args={})),required=False)
            if reply and predicate(reply['data']): return reply['data']
            time.sleep(.1)
        raise RuntimeError('Host state timeout: '+str(reply))
    def capture(name):
        path=OUTPUT/(name.replace('.png','-dark.png') if DARK else name)
        if path.exists(): path.unlink()
        rpc('window.capture',path=str(path))
    try:
        wait(lambda value:value['hidden'])
        rpc('page.select',id='news')
        rpc('window.show')
        wait(lambda value:not value['busy'] and value['news_sources']==24)
        rpc('settings.open',category='page')
        wait(lambda value:not value['busy'] and value['section']=='Settings(Page)')
        capture('settings-wide.png')
        rpc('modules.toggle')
        wait(lambda value:value['section']=='Modules' and not value['manager_busy'])
        assert children(host.pid)==set()
        capture('modules-wide.png')
        rpc('modules.toggle')
        wait(lambda value:value['section']=='Settings(Page)' and not value['busy'])
        windows=[]
        CALLBACK=ctypes.WINFUNCTYPE(wintypes.BOOL,wintypes.HWND,wintypes.LPARAM)
        user32.GetWindowThreadProcessId.argtypes=[wintypes.HWND,ctypes.POINTER(wintypes.DWORD)]
        user32.IsWindowVisible.argtypes=[wintypes.HWND]
        user32.GetPropW.argtypes=[wintypes.HWND,wintypes.LPCWSTR]
        user32.GetPropW.restype=wintypes.HANDLE
        user32.EnumWindows.argtypes=[CALLBACK,wintypes.LPARAM]
        @CALLBACK
        def visit(handle,_):
            pid=wintypes.DWORD(); user32.GetWindowThreadProcessId(handle,ctypes.byref(pid))
            if pid.value==host.pid and user32.GetPropW(handle,"LazyBuddy.Window"): windows.append(handle)
            return True
        user32.EnumWindows(visit,0)
        assert len(windows)==1, windows
        window=windows[0]
        gdi=ctypes.WinDLL('gdi32',use_last_error=True)
        gdi.CreateRectRgn.argtypes=[ctypes.c_int]*4;gdi.CreateRectRgn.restype=wintypes.HANDLE
        gdi.PtInRegion.argtypes=[wintypes.HANDLE,ctypes.c_int,ctypes.c_int]
        gdi.DeleteObject.argtypes=[wintypes.HANDLE]
        user32.GetWindowRgn.argtypes=[wintypes.HWND,wintypes.HANDLE]
        region=gdi.CreateRectRgn(0,0,0,0)
        try:
            assert user32.GetWindowRgn(window,region)>0
            assert gdi.PtInRegion(region,0,0), 'Window corner is clipped'
        finally:gdi.DeleteObject(region)
        user32.SetWindowPos.argtypes=[wintypes.HWND,wintypes.HWND,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,wintypes.UINT]
        assert user32.SetWindowPos(window,None,170,180,460,560,0x14)
        wait(lambda value:value['window'][2]<640)
        time.sleep(.2)
        capture('settings-narrow.png')
        rpc('modules.toggle')
        wait(lambda value:value['section']=='Modules' and not value['manager_busy'])
        capture('modules-narrow.png')
        rpc('modules.toggle')
        rpc('settings.back')
        wait(lambda value:value['section']=='Content' and not value['busy'])
        rpc('modules.toggle');rpc('modules.toggle')
        assert rpc('status')['section']=='Content'
        foreground_pid=wintypes.DWORD()
        user32.GetWindowThreadProcessId(user32.GetForegroundWindow(),ctypes.byref(foreground_pid))
        assert foreground_pid.value!=host.pid, 'Isolated host took foreground' 
        rpc('app.quit');host.wait(timeout=15)
        report=dict(passed=True,settings_return=True,content_return=True,rectangular_region=True,sources=24,wide_narrow_captured=True,real_services=False,host_did_not_take_focus=True,visual_checked=False)
        (OUTPUT/('shell-dark.json' if DARK else 'shell.json')).write_text(json.dumps(report,indent=2),encoding='utf-8')
        print(json.dumps(report))
    finally:
        if host.poll() is None: rpc('app.quit');host.wait(timeout=15)
