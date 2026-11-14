"""Keep the release identity outside the checkout, protected by current-user DPAPI."""
import ctypes
from ctypes import wintypes
import os
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


def protect(payload, *, decrypt=False):
    if os.name != 'nt':
        raise OSError('DPAPI signing keys require Windows')

    class Blob(ctypes.Structure):
        _fields_ = [('size', wintypes.DWORD), ('data', ctypes.POINTER(ctypes.c_ubyte))]

    buffer = (ctypes.c_ubyte * len(payload)).from_buffer_copy(payload)
    source = Blob(len(payload), buffer)
    destination = Blob()
    crypt = ctypes.WinDLL('crypt32', use_last_error=True)
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    operation = crypt.CryptUnprotectData if decrypt else crypt.CryptProtectData
    operation.argtypes = [ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p,
                          ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    operation.restype = wintypes.BOOL
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    kernel.LocalFree.restype = ctypes.c_void_p
    if not operation(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(destination)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        return ctypes.string_at(destination.data, destination.size)
    finally:
        ctypes.memset(destination.data, 0, destination.size)
        kernel.LocalFree(destination.data)


def load_key(path, *, initialize=False):
    path = Path(path).resolve()
    checkout = Path(__file__).resolve().parents[1]
    if path.is_relative_to(checkout):
        raise ValueError('release private key must be outside the checkout')
    if initialize:
        key = Ed25519PrivateKey.generate()
        protected = protect(key.private_bytes_raw())
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as output:
            output.write(protected)
    return Ed25519PrivateKey.from_private_bytes(protect(path.read_bytes(), decrypt=True))
