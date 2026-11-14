"""Offline module/API boundary regression; no GUI, credentials or news requests."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / 'target/debug/lazybuddy-cli.exe'
SERVICE = ROOT / 'target/debug/examples/fixture.exe'


class ModuleTests(unittest.TestCase):
    def test_install_call_remove_and_missing_entry_rollback(self):
        with tempfile.TemporaryDirectory(prefix='qa-module-', dir=ROOT / 'target') as temporary:
            directory = Path(temporary)
            package = directory / 'package'
            (package / 'bin').mkdir(parents=True)
            shutil.copyfile(ROOT / 'contracts/fixtures/feature.json', package / 'module.json')
            environment = dict(os.environ, LAZYBUDDY_DATA_DIR=str(directory / 'data'))

            def run(*args, success=True):
                result = subprocess.run([str(CLI), *args], env=environment, capture_output=True,
                                        text=True, encoding='utf-8', timeout=15,
                                        creationflags=subprocess.CREATE_NO_WINDOW)
                self.assertEqual(result.returncode == 0, success, result.stderr)
                return json.loads(result.stdout) if success else None

            run('modules', 'install-local', str(package), success=False)
            self.assertEqual(run('modules', 'list'), [])
            shutil.copyfile(SERVICE, package / 'bin/service.exe')
            run('modules', 'install-local', str(package))
            self.assertEqual(len(run('modules', 'list')), 1)
            request = json.dumps(dict(version=1, id='catalog', method='sources'))
            reply = run('call', 'sample', request)
            self.assertTrue(reply['ok'])
            ids = [row['id'] for row in reply['data']]
            self.assertEqual(len(ids), len(set(ids)))
            self.assertGreater(len(ids), 0)
            run('modules', 'uninstall', 'sample')
            self.assertEqual(run('modules', 'list'), [])
            run('call', 'sample', request, success=False)


if __name__ == '__main__':
    unittest.main()
