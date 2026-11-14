"""One synthetic GPU IPC handshake, no window/audio/account/media network."""
import ctypes
import os
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
GST = ROOT / 'target/native-player-sdk/PFiles64/gstreamer/1.0/msvc_x86_64'
pipe = r'\\.\pipe\lazybuddy-media-' + secrets.token_hex(8)
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
kernel.WaitNamedPipeW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32]
with tempfile.TemporaryDirectory(prefix='qa-media-ipc-', dir=ROOT / 'target') as temporary:
    environment = dict(os.environ, PATH=str(GST / 'bin') + ';' + os.environ['PATH'],
        GST_PLUGIN_SYSTEM_PATH_1_0=str(GST / 'lib/gstreamer-1.0'),
        GST_PLUGIN_SCANNER_1_0=str(GST / 'libexec/gstreamer-1.0/gst-plugin-scanner.exe'),
        GST_REGISTRY=str(Path(temporary) / 'registry.bin'), GST_DEBUG='0')
    diagnostics = open(Path(temporary) / 'producer.log', 'wb')
    producer = subprocess.Popen([str(GST / 'bin/gst-launch-1.0.exe'), '-q',
        'videotestsrc', 'pattern=red', 'is-live=true', 'num-buffers=60', '!',
        'video/x-raw,format=RGBA,width=320,height=180,framerate=30/1', '!',
        'd3d12upload', '!', 'd3d12ipcsink', 'pipe-name="' + pipe.replace('\\', '\\\\') + '"'], env=environment,
        stdout=subprocess.DEVNULL, stderr=diagnostics, creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        deadline = time.monotonic() + 20
        while not kernel.WaitNamedPipeW(pipe, 100):
            if producer.poll() is not None:
                diagnostics.flush()
                diagnostic = (Path(temporary) / 'producer.log').read_text(encoding='utf-8', errors='replace')[-6000:]
                raise RuntimeError('synthetic producer exited before IPC configuration: ' + diagnostic)
            if time.monotonic() >= deadline:
                raise RuntimeError('synthetic producer IPC startup timeout')
            time.sleep(0.05)
        rendering = '--render' in sys.argv
        executable = 'render_probe.exe' if rendering else 'probe.exe'
        result = subprocess.run([str(ROOT / 'target/debug/examples' / executable), pipe, str(producer.pid)],
            capture_output=True, text=True, timeout=15, creationflags=subprocess.CREATE_NO_WINDOW)
        if result.returncode:
            raise RuntimeError('IPC metadata probe failed: ' + result.stderr)
        print(result.stdout, end='')
        if rendering:
            if result.stdout.count('production_primitive_verified') != 6:
                raise RuntimeError('expected both target formats at three opacity levels')
        elif result.stdout.count('frame_metadata_received') != 3:
            raise RuntimeError('expected three exported frame metadata packets')
    finally:
        producer.terminate()
        producer.wait(timeout=5)
        diagnostics.close()
