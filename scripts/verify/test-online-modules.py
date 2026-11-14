"""Opt-in HTTPS release check: real in-app install/remove, no service/account requests."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host-dir', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    binaries = args.host_dir.resolve()
    source = json.loads((binaries / 'module-catalog.json').read_text(encoding='utf-8'))
    with tempfile.TemporaryDirectory(prefix='qa-online-modules-', dir=ROOT / 'target') as temporary:
        env = dict(os.environ, LAZYBUDDY_INSTANCE='qa-modules-' + uuid.uuid4().hex,
                   LAZYBUDDY_DATA_DIR=str(Path(temporary) / 'data'))
        host = subprocess.Popen([str(binaries / 'lazybuddy.exe'), '--background'], env=env,
                                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)

        def rpc(command, required=True, **values):
            result = subprocess.run([str(binaries / 'lazybuddy-cli.exe'), 'host',
                                     json.dumps(dict(version=1, command=command, args=values))],
                                    env=env, capture_output=True, encoding='utf-8', timeout=15,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
            if result.returncode:
                if required:
                    raise RuntimeError('host unavailable')
                return None
            reply = json.loads(result.stdout)
            if required and not reply['ok']:
                raise RuntimeError(f'{command} rejected: {reply}')
            return reply

        def wait(predicate, timeout=600):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if host.poll() is not None:
                    raise RuntimeError('host exited')
                reply = rpc('status', required=False)
                if reply and reply.get('ok') and predicate(reply['data']):
                    return reply['data']
                if reply and reply.get('ok') and reply['data']['error']:
                    raise RuntimeError('host operation failed: ' + str(reply['data']['error']))
                time.sleep(.2)
            raise RuntimeError('state timeout: ' + str(reply))

        try:
            initial = wait(lambda value: value['hidden'], timeout=20)
            assert initial['modules'] == {}
            assert not rpc('modules.install', required=False, id='news')['ok']
            rpc('window.show')
            rpc('modules.open')
            ready = wait(lambda value: not value['manager_busy'] and value['available_modules'] is not None)
            assert ready['error'] is None, ready
            assert set(ready['available_modules']) == {'news', 'hongguo'}
            assert not rpc('modules.install', required=False, id='unknown')['ok']
            for identifier, expected in [('news', {'news'}),
                                         ('hongguo', {'news', 'hongguo', 'python-runtime', 'media-runtime'})]:
                print('Installing ' + identifier, flush=True)
                rpc('modules.install', id=identifier)
                status = wait(lambda value: not value['manager_busy'])
                assert status['error'] is None, status
                assert set(status['modules']) == expected, status
                assert status['service_active'] is False
            for identifier, expected in [('news', {'hongguo', 'python-runtime', 'media-runtime'}),
                                         ('hongguo', set())]:
                rpc('modules.uninstall', id=identifier)
                status = wait(lambda value: not value['manager_busy'])
                assert status['error'] is None, status
                assert set(status['modules']) == expected, status
            report = dict(passed=True, catalog_url=source['url'], same_process=True,
                          installed=['news', 'hongguo', 'python-runtime', 'media-runtime'],
                          runtime_cleanup=True, service_started=False, real_account_tested=False)
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2), encoding='utf-8')
            print(json.dumps(report))
        finally:
            if host.poll() is None:
                rpc('app.quit')
                host.wait(timeout=20)


if __name__ == '__main__':
    main()
