"""Capture only the owned prototype client, including native composition."""
import argparse
import ctypes as c
from ctypes import wintypes as w
from PIL import Image

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--pid',type=int,required=True)
p.add_argument('--output',required=True)
a=p.parse_args()
u=c.WinDLL('user32',use_last_error=True);g=c.WinDLL('gdi32',use_last_error=True)
callback=c.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
u.EnumWindows.argtypes=[callback,w.LPARAM]
u.GetWindowThreadProcessId.argtypes=[w.HWND,c.POINTER(w.DWORD)]
u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,c.c_int]
windows=[]
@callback
def visit(hwnd,_):
    pid=w.DWORD();u.GetWindowThreadProcessId(hwnd,c.byref(pid))
    title=c.create_unicode_buffer(256);u.GetWindowTextW(hwnd,title,len(title))
    if pid.value==a.pid and title.value.startswith('LazyBuddy'):windows.append(hwnd)
    return True
u.EnumWindows(visit,0)
if len(windows)!=1:raise RuntimeError('owned prototype window not unique')
hwnd=windows[0]
u.GetClientRect.argtypes=[w.HWND,c.POINTER(w.RECT)]
u.GetDC.argtypes=[w.HWND];u.GetDC.restype=w.HDC
u.ReleaseDC.argtypes=[w.HWND,w.HDC]
u.PrintWindow.argtypes=[w.HWND,w.HDC,w.UINT]
g.CreateCompatibleDC.argtypes=[w.HDC];g.CreateCompatibleDC.restype=w.HDC
g.CreateCompatibleBitmap.argtypes=[w.HDC,c.c_int,c.c_int];g.CreateCompatibleBitmap.restype=w.HBITMAP
g.SelectObject.argtypes=[w.HDC,w.HANDLE];g.SelectObject.restype=w.HANDLE
g.DeleteObject.argtypes=[w.HANDLE];g.DeleteDC.argtypes=[w.HDC]
class Header(c.Structure):
    _fields_=[('size',w.DWORD),('width',w.LONG),('height',w.LONG),('planes',w.WORD),('bits',w.WORD),('compression',w.DWORD),('image',w.DWORD),('x',w.LONG),('y',w.LONG),('colors',w.DWORD),('important',w.DWORD)]
g.GetDIBits.argtypes=[w.HDC,w.HBITMAP,w.UINT,w.UINT,c.c_void_p,c.POINTER(Header),w.UINT]
rect=w.RECT();u.GetClientRect(hwnd,c.byref(rect));width,height=rect.right,rect.bottom
dc=u.GetDC(hwnd);memory=g.CreateCompatibleDC(dc);bitmap=g.CreateCompatibleBitmap(dc,width,height);old=g.SelectObject(memory,bitmap)
try:
    if not u.PrintWindow(hwnd,memory,3):raise c.WinError(c.get_last_error())
    g.SelectObject(memory,old)
    header=Header(c.sizeof(Header),width,-height,1,32,0,0,0,0,0,0)
    pixels=c.create_string_buffer(width*height*4)
    if g.GetDIBits(memory,bitmap,0,height,pixels,c.byref(header),0)!=height:raise c.WinError(c.get_last_error())
    Image.frombytes('RGB',(width,height),pixels.raw,'raw','BGRX').save(a.output)
finally:
    g.SelectObject(memory,old);g.DeleteObject(bitmap);g.DeleteDC(memory);u.ReleaseDC(hwnd,dc)
