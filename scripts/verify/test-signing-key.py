"""DPAPI identity reuse, overwrite protection and tamper rejection; no release key access."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from windows_signing_key import load_key, protect


class SigningKey(unittest.TestCase):
    def test_reuse_and_reject_overwrite_or_corruption(self):
        with tempfile.TemporaryDirectory(prefix='lazybuddy-signing-test-') as directory:
            path = Path(directory) / 'test.dpapi'
            first = load_key(path, initialize=True)
            second = load_key(path)
            second.public_key().verify(first.sign(b'catalog'), b'catalog')
            with self.assertRaises(FileExistsError):
                load_key(path, initialize=True)
            corrupted = bytearray(path.read_bytes())
            corrupted[-1] ^= 1
            with self.assertRaises(OSError):
                protect(bytes(corrupted), decrypt=True)

    def test_reject_checkout_private_key(self):
        with self.assertRaises(ValueError):
            load_key(Path(__file__).resolve().parents[2] / 'target' / 'forbidden.dpapi')


if __name__ == '__main__':
    unittest.main()
