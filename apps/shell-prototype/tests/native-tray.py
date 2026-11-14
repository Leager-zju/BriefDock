"""Send close/tray messages only to the explicitly owned preview, without mouse input."""
import argparse,ctypes,json
from ctypes import wintypes as w
from pathlib import Path

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--pid',type=int,required=True)
p.add_argument('--exe',type=Path,required=True)
p.add_argument('--action',choices=['snapshot','close','restore'],required=True)
args=p.parse_args()
user=ctypes.WinDLL('user32',use_last_error=True)
kernel=ctypes.WinDLL('kernel32',use_last_error=True)
kernel.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];kernel.OpenProcess.restype=w.HANDLE
kernel.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)]
kernel.CloseHandle.argtypes=[w.HANDLE]
handle=kernel.OpenProcess(0x1000,False,args.pid)
if not handle:raise ctypes.WinError(ctypes.get_last_error())
try:
    path=ctypes.create_unicode_buffer(32768);length=w.DWORD(len(path))
    if not kernel.QueryFullProcessImageNameW(handle,0,path,ctypes.byref(length)):raise ctypes.WinError(ctypes.get_last_error())
    if Path(path.value).resolve()!=args.exe.resolve():raise ValueError('unexpected process executable')
finally:kernel.CloseHandle(handle)
callback=ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
user.EnumWindows.argtypes=[callback,w.LPARAM]
user.GetWindowThreadProcessId.argtypes=[w.HWND,ctypes.POINTER(w.DWORD)]
user.GetClassNameW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int]
user.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int]
windows=[]
@callback
def collect(hwnd,_):
    pid=w.DWORD();user.GetWindowThreadProcessId(hwnd,ctypes.byref(pid))
    if pid.value==args.pid:
        value=ctypes.create_unicode_buffer(256)
        if args.action=='restore':
            user.GetClassNameW(hwnd,value,len(value));matches=value.value=='tray_icon_app'
        else:
            user.GetWindowTextW(hwnd,value,len(value));matches=value.value.startswith('LazyBuddy')
        if matches:windows.append(hwnd)
    return True
user.EnumWindows(collect,0)
if len(windows)!=1:raise ValueError('owned window not uniquely identified')
user.PostMessageW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM]
user.IsWindowVisible.argtypes=[w.HWND]
user.IsIconic.argtypes=[w.HWND]
if args.action=='snapshot':
    print(json.dumps(dict(visible=bool(user.IsWindowVisible(windows[0])),minimized=bool(user.IsIconic(windows[0])))))
    raise SystemExit(0)
# tray-icon 0.25.1's pinned Windows notification callback and left-button-up event.
message,parameter=(6002,0x202) if args.action=='restore' else (0x10,0)
if not user.PostMessageW(windows[0],message,0,parameter):raise ctypes.WinError(ctypes.get_last_error())
