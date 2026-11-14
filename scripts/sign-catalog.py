"""Sign exact catalog bytes with a maintainer Ed25519 identity."""
import argparse
import json
from pathlib import Path
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--catalog', type=Path, required=True)
    keys = parser.add_mutually_exclusive_group(required=True)
    keys.add_argument('--key', type=Path, help='existing private PEM file')
    keys.add_argument('--protected-key', type=Path, help='current-user Windows DPAPI file outside checkout')
    parser.add_argument('--initialize-key', action='store_true', help='explicitly create a new protected identity; refuses overwrite')
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--url', required=True)
    args = parser.parse_args()
    if not args.url.startswith('https://'):
        raise ValueError('HTTPS catalog URL required')
    signature_path = args.catalog.with_name(args.catalog.name + '.sig')
    if signature_path.exists() or args.source.exists():
        raise ValueError('signature and trusted source output must not exist')
    payload = args.catalog.read_bytes()
    catalog = json.loads(payload)
    if len(payload) > 1024 * 1024 or catalog.get('schema') != 1 or not isinstance(catalog.get('packages'), list):
        raise ValueError('invalid catalog')
    if args.initialize_key and not args.protected_key:
        parser.error('--initialize-key requires --protected-key')
    if args.protected_key:
        from windows_signing_key import load_key
        key = load_key(args.protected_key, initialize=args.initialize_key)
    else:
        key = serialization.load_pem_private_key(args.key.read_bytes(), password=None)
    if not isinstance(key, Ed25519PrivateKey):
        raise ValueError('Ed25519 key required')
    signature = key.sign(payload)
    key.public_key().verify(signature, payload)
    with signature_path.open('xb') as file:
        file.write(signature)
    source = dict(url=args.url, public_key=list(key.public_key().public_bytes_raw()))
    with args.source.open('x', encoding='utf-8') as file:
        json.dump(source, file, indent=2)
    print(json.dumps(dict(signed=True, packages=len(catalog['packages']))))


if __name__ == '__main__':
    main()
