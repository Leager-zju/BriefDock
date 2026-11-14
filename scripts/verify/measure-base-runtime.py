"""Short idle baseline of an isolated packaged host; no input, focus or upstream requests."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid
from windows_fixture import kernel, user32, descendants, executable

ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'target/base-dev/LazyBuddy'
before=user32.GetForegroundWindow()


class Memory(ctypes.Structure):
    _fields_=[('size',wintypes.DWORD),('faults',wintypes.DWORD),('peak_working',ctypes.c_size_t),('working',ctypes.c_size_t),('peak_paged',ctypes.c_size_t),('paged',ctypes.c_size_t),('peak_nonpaged',ctypes.c_size_t),('nonpaged',ctypes.c_size_t),('pagefile',ctypes.c_size_t),('peak_pagefile',ctypes.c_size_t),('private',ctypes.c_size_t)]


kernel.GetProcessTimes.argtypes=[wintypes.HANDLE,*([ctypes.POINTER(wintypes.FILETIME)]*4)]
kernel.K32GetProcessMemoryInfo.argtypes=[wintypes.HANDLE,ctypes.POINTER(Memory),wintypes.DWORD]


def metrics(pid):
    handle=kernel.OpenProcess(0x1000|0x10,False,pid)
    if not handle:raise ctypes.WinError(ctypes.get_last_error())
    try:
        times=[wintypes.FILETIME() for _ in range(4)]
        if not kernel.GetProcessTimes(handle,*(ctypes.byref(value) for value in times)):raise ctypes.WinError(ctypes.get_last_error())
        memory=Memory(size=ctypes.sizeof(Memory))
        if not kernel.K32GetProcessMemoryInfo(handle,ctypes.byref(memory),memory.size):raise ctypes.WinError(ctypes.get_last_error())
        cpu=sum((value.dwHighDateTime<<32)+value.dwLowDateTime for value in times[2:])/10_000_000
        return dict(cpu_seconds=cpu,private_bytes=memory.private,working_set_bytes=memory.working)
    finally:kernel.CloseHandle(handle)


with tempfile.TemporaryDirectory(prefix='qa-base-metrics-',dir=ROOT/'target') as temporary:
    directory=Path(temporary)
    environment=dict(os.environ,LAZYBUDDY_INSTANCE='qa-metrics-'+uuid.uuid4().hex,LAZYBUDDY_DATA_DIR=temporary,
        PATH=os.environ['SystemRoot']+'\\System32',PYTHONPATH='')
    def cli(*args,required=True):
        result=subprocess.run([str(BASE/'lazybuddy-cli.exe'),*args],env=environment,capture_output=True,encoding='utf-8',timeout=10,creationflags=subprocess.CREATE_NO_WINDOW)
        if user32.GetForegroundWindow()!=before:raise RuntimeError('foreground changed')
        if result.returncode:
            if required:raise RuntimeError('CLI request failed')
            return None
        return json.loads(result.stdout)
    cli('modules','install-local',str(ROOT/'target/packages-dev/news'))
    cli('call','news',json.dumps(dict(version=1,id='metrics-settings',method='configure',args=dict(preferences=dict(auto_refresh=False,refresh_minutes=5,followed=[],hidden=[],order=[],default_followed=False)))))
    started=time.monotonic()
    host=subprocess.Popen([str(BASE/'lazybuddy.exe'),'--background'],env=environment,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        def rpc(command,**args):return cli('host',json.dumps(dict(version=1,command=command,args=args)))
        def wait(predicate):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                if host.poll() is not None:raise RuntimeError('host exited')
                reply=cli('host','{"version":1,"command":"status"}',required=False)
                if reply and predicate(reply['data']):return reply['data']
                time.sleep(.05)
            raise RuntimeError('host readiness timeout')
        wait(lambda value:value['hidden'] and value['window'])
        ready=time.monotonic()-started
        def sample(label):
            time.sleep(.5)
            owned={host.pid,*descendants(host.pid)}
            initial={pid:metrics(pid) for pid in owned};start=time.monotonic();time.sleep(3)
            assert {host.pid,*descendants(host.pid)}==owned
            final={pid:metrics(pid) for pid in owned};elapsed=time.monotonic()-start
            return dict(state=label,processes=len(owned),process_names=sorted(Path(executable(pid)).name for pid in owned),cpu_percent_one_core=round(sum(final[pid]['cpu_seconds']-initial[pid]['cpu_seconds'] for pid in owned)/elapsed*100,3),
                private_mib=round(sum(row['private_bytes'] for row in final.values())/1048576,2),working_set_mib=round(sum(row['working_set_bytes'] for row in final.values())/1048576,2),sample_seconds=round(elapsed,2))
        rows=[sample('hidden')]
        rpc('window.show');wait(lambda value:not value['hidden'] and not value['busy'])
        rows.append(sample('empty_host_visible'))
        rpc('page.select',id='news');wait(lambda value:value['service_active'] and value['news_sources']==53 and not value['busy'])
        rows.append(sample('news_visible_no_refresh'))
        rpc('settings.open',category='about');wait(lambda value:not value['service_active'])
        rows.append(sample('global_settings'))
        rpc('window.hide');wait(lambda value:value['hidden']);rows.append(sample('hidden_after_news'))
        rpc('app.quit');host.wait(10);assert not descendants(host.pid)
        result=dict(hidden_ready_seconds=round(ready,3),foreground_unchanged=user32.GetForegroundWindow()==before,upstream_requests=False,
            gpu_memory_measured=False,long_term_leak_checked=False,samples=rows)
        output=ROOT/'target/verification-history/lazybuddy-runtime.json';output.parent.mkdir(exist_ok=True)
        output.write_text(json.dumps(result,indent=2),encoding='utf-8');print(json.dumps(result))
    finally:
        if host.poll() is None:host.terminate();host.wait(5)
