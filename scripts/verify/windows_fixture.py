"""Read-only Windows helpers for isolated host checks."""
import ctypes
from ctypes import wintypes

user32=ctypes.WinDLL('user32',use_last_error=True)
user32.GetForegroundWindow.restype=wintypes.HWND
kernel=ctypes.WinDLL('kernel32',use_last_error=True)


class Process(ctypes.Structure):
    _fields_=[('size',wintypes.DWORD),('usage',wintypes.DWORD),('pid',wintypes.DWORD),
        ('heap',ctypes.c_size_t),('module',wintypes.DWORD),('threads',wintypes.DWORD),
        ('parent',wintypes.DWORD),('priority',wintypes.LONG),('flags',wintypes.DWORD),('name',wintypes.WCHAR*260)]


kernel.CreateToolhelp32Snapshot.argtypes=[wintypes.DWORD,wintypes.DWORD]
kernel.CreateToolhelp32Snapshot.restype=wintypes.HANDLE
kernel.Process32FirstW.argtypes=[wintypes.HANDLE,ctypes.POINTER(Process)]
kernel.Process32NextW.argtypes=[wintypes.HANDLE,ctypes.POINTER(Process)]
kernel.CloseHandle.argtypes=[wintypes.HANDLE]
kernel.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
kernel.OpenProcess.restype=wintypes.HANDLE
kernel.QueryFullProcessImageNameW.argtypes=[wintypes.HANDLE,wintypes.DWORD,wintypes.LPWSTR,ctypes.POINTER(wintypes.DWORD)]

def executable(pid):
    handle=kernel.OpenProcess(0x1000,False,pid)
    if not handle:return ''
    try:
        text=ctypes.create_unicode_buffer(32768);size=wintypes.DWORD(len(text))
        return text.value if kernel.QueryFullProcessImageNameW(handle,0,text,ctypes.byref(size)) else ''
    finally:kernel.CloseHandle(handle)


def children(parent):
    snapshot=kernel.CreateToolhelp32Snapshot(2,0)
    if snapshot==ctypes.c_void_p(-1).value:raise ctypes.WinError(ctypes.get_last_error())
    try:
        entry=Process(size=ctypes.sizeof(Process));rows=[]
        more=kernel.Process32FirstW(snapshot,ctypes.byref(entry))
        while more:
            rows.append((entry.pid,entry.parent));more=kernel.Process32NextW(snapshot,ctypes.byref(entry))
        return {pid for pid,owner in rows if owner==parent}
    finally:kernel.CloseHandle(snapshot)


def descendants(parent):
    result=set();pending=[parent]
    while pending:
        nested=children(pending.pop())-result
        result.update(nested);pending.extend(nested)
    return result
