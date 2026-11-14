"""Exercise only our owned prototype's native move messages; never move the mouse."""
import argparse
import ctypes
import json
from ctypes import wintypes as w
from pathlib import PureWindowsPath

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--pid', type=int, required=True)
parser.add_argument('--action', choices=['snapshot', 'drag-left', 'drag-right', 'drag-center', 'resize-left'], default='snapshot')
args = parser.parse_args()
user = ctypes.WinDLL('user32', use_last_error=True)
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
gdi = ctypes.WinDLL('gdi32', use_last_error=True)
kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
kernel.OpenProcess.restype = w.HANDLE
kernel.QueryFullProcessImageNameW.argtypes = [w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)]
kernel.CloseHandle.argtypes = [w.HANDLE]
handle = kernel.OpenProcess(0x1000, False, args.pid)
if not handle: raise ctypes.WinError(ctypes.get_last_error())
try:
    name = ctypes.create_unicode_buffer(32768); size = w.DWORD(len(name))
    if not kernel.QueryFullProcessImageNameW(handle, 0, name, ctypes.byref(size)): raise ctypes.WinError(ctypes.get_last_error())
    if PureWindowsPath(name.value).name != 'lazybuddy-shell-prototype.exe': raise ValueError('not the owned prototype')
finally: kernel.CloseHandle(handle)

callback_type = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
user.EnumWindows.argtypes = [callback_type, w.LPARAM]
user.GetWindowThreadProcessId.argtypes = [w.HWND, ctypes.POINTER(w.DWORD)]
user.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, ctypes.c_int]
windows = []
@callback_type
def collect(hwnd, parameter):
    pid = w.DWORD(); user.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    if pid.value == args.pid:
        title = ctypes.create_unicode_buffer(256); user.GetWindowTextW(hwnd, title, len(title))
        if title.value.startswith('LazyBuddy'): windows.append(hwnd)
    return True
user.EnumWindows(collect, 0)
if len(windows) != 1: raise ValueError('prototype main window not uniquely identified')
hwnd = windows[0]
user.GetWindowRect.argtypes = [w.HWND, ctypes.POINTER(w.RECT)]
user.GetDpiForWindow.argtypes = [w.HWND]
user.GetWindowLongPtrW.argtypes = [w.HWND, ctypes.c_int]; user.GetWindowLongPtrW.restype = ctypes.c_ssize_t
user.MonitorFromWindow.argtypes = [w.HWND, w.DWORD]; user.MonitorFromWindow.restype = w.HANDLE
class MonitorInfo(ctypes.Structure):
    _fields_ = [('size', w.DWORD), ('monitor', w.RECT), ('work', w.RECT), ('flags', w.DWORD)]
user.GetMonitorInfoW.argtypes = [w.HANDLE, ctypes.POINTER(MonitorInfo)]
info = MonitorInfo(); info.size = ctypes.sizeof(info)
if not user.GetMonitorInfoW(user.MonitorFromWindow(hwnd, 2), ctypes.byref(info)): raise ctypes.WinError(ctypes.get_last_error())
def bounds():
    rect = w.RECT()
    if not user.GetWindowRect(hwnd, ctypes.byref(rect)): raise ctypes.WinError(ctypes.get_last_error())
    return [rect.left, rect.top, rect.right-rect.left, rect.bottom-rect.top]
before = bounds(); before_exit = None
if args.action != 'snapshot':
    user.SetWindowPos.argtypes = [w.HWND, w.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.UINT]
    user.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, ctypes.POINTER(ctypes.c_size_t)]
    user.SendMessageTimeoutW.restype = ctypes.c_ssize_t
    def send(message, parameter=0):
        result = ctypes.c_size_t()
        if not user.SendMessageTimeoutW(hwnd, message, 0, parameter, 2, 5000, ctypes.byref(result)): raise ctypes.WinError(ctypes.get_last_error())
    send(0x231)
    width = before[2] - (4 if args.action == 'resize-left' else 0)
    x = info.work.left if args.action.endswith('left') else info.work.right-width if args.action == 'drag-right' else (info.work.left+info.work.right-width)//2
    if not user.SetWindowPos(hwnd, None, x, before[1], width, before[3], 0x14): raise ctypes.WinError(ctypes.get_last_error())
    if args.action != 'resize-left':
        proposed = w.RECT(x,before[1],x+width,before[1]+before[3])
        send(0x216, ctypes.addressof(proposed))
    before_exit = bounds()
    send(0x232)

gdi.CreateRectRgn.argtypes = [ctypes.c_int]*4; gdi.CreateRectRgn.restype = w.HANDLE
gdi.DeleteObject.argtypes = [w.HANDLE]
gdi.PtInRegion.argtypes = [w.HANDLE, ctypes.c_int, ctypes.c_int]
user.GetWindowRgn.argtypes = [w.HWND, w.HANDLE]
after = bounds(); region = gdi.CreateRectRgn(0,0,0,0)
user.WindowFromPoint.argtypes = [w.POINT]; user.WindowFromPoint.restype = w.HWND
user.GetAncestor.argtypes = [w.HWND, w.UINT]; user.GetAncestor.restype = w.HWND
def hits_own_window(x, y):
    return user.GetAncestor(user.WindowFromPoint(w.POINT(x,y)),2) == hwnd
try:
    region_type = user.GetWindowRgn(hwnd,region)
    shape = dict(type=region_type,corner=bool(gdi.PtInRegion(region,0,0)),center=bool(gdi.PtInRegion(region,after[2]//2,after[3]//2)),corner_hits_prototype=hits_own_window(after[0],after[1]),center_hits_prototype=hits_own_window(after[0]+after[2]//2,after[1]+after[3]//2))
finally: gdi.DeleteObject(region)
print(json.dumps(dict(before=before,before_exit=before_exit,after=after,dpi=user.GetDpiForWindow(hwnd),region=shape,topmost=bool(user.GetWindowLongPtrW(hwnd,-20)&8))))
