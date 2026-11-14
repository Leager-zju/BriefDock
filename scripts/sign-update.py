"""Sign application metadata with the existing protected maintainer identity."""
import argparse
import json
from pathlib import Path
from windows_signing_key import load_key


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--protected-key', type=Path, required=True)
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[1]/'app-update-source.json')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--replace', action='store_true')
    args = parser.parse_args()
    signature_path = args.output.with_name(args.output.name + '.sig')
    if not args.replace and (args.output.exists() or signature_path.exists()):
        raise ValueError('update metadata exists; use --replace for the test channel')
    payload = args.manifest.read_bytes()
    manifest = json.loads(payload)
    if len(payload) > 1024*1024 or manifest['schema'] != 1 or manifest['target'] != 'windows-x86_64':
        raise ValueError('unsupported update payload')
    key = load_key(args.protected_key)
    source = json.loads(args.source.read_text(encoding='utf-8'))
    if list(key.public_key().public_bytes_raw()) != source['public_key']:
        raise ValueError('maintainer identity does not match the shipped trust root')
    signature = key.sign(payload)
    key.public_key().verify(signature, payload)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(payload)
    signature_path.write_bytes(signature)
    print(json.dumps(dict(signed=True, build=manifest['build'])))


if __name__ == '__main__':
    main()
