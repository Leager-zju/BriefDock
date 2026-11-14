"""One hidden, isolated host startup; does not claim visual or interaction QA."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
user32 = ctypes.WinDLL('user32', use_last_error=True)
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
before = user32.GetForegroundWindow()
with tempfile.TemporaryDirectory(prefix='qa-host-', dir=ROOT / 'target') as temporary:
    process = subprocess.Popen([str(ROOT / 'target/debug/lazybuddy.exe'), '--background'],
        env=dict(os.environ, LAZYBUDDY_DATA_DIR=temporary, LAZYBUDDY_INSTANCE='qa-host'), stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        windows = []

        @callback_type
        def inspect(window, _):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(window, ctypes.byref(owner))
            if owner.value == process.pid:
                title = ctypes.create_unicode_buffer(256)
                user32.GetWindowTextW(window, title, len(title))
                if title.value == 'LazyBuddy':
                    windows.append(bool(user32.IsWindowVisible(window)))
            return True

        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError('hidden host exited during startup')
            windows.clear()
            user32.EnumWindows(inspect, 0)
            if windows:
                break
            time.sleep(0.1)
        if not windows or any(windows):
            raise RuntimeError('expected only hidden host windows')
        print(json.dumps(dict(hidden_window_created=True, foreground_unchanged=user32.GetForegroundWindow() == before,
                             installed_modules=0, visual_checked=False)))
    finally:
        process.terminate()
        process.wait(timeout=5)
