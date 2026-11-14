"""Application release metadata detects replaced test builds and damaged bundles."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('base', ROOT/'scripts/package_base.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)


class Release(unittest.TestCase):
    def bundle(self, directory, marker, missing=False, corrupt=False):
        names = ['lazybuddy.exe', 'lazybuddy-cli.exe', 'lazybuddy-updater.exe', 'app-update-source.json', 'module-catalog.json']
        if missing:
            names.remove('lazybuddy-updater.exe')
        data = {name: (marker+name).encode() for name in names}
        files = {name: hashlib.sha256(value).hexdigest() for name,value in data.items()}
        build = hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':')).encode()).hexdigest()
        path = directory/(marker+'.zip')
        with zipfile.ZipFile(path,'w') as archive:
            for name,value in data.items():
                archive.writestr('LazyBuddy/'+name, b'damaged' if corrupt and name=='lazybuddy.exe' else value)
            archive.writestr('LazyBuddy/build-info.json',json.dumps(dict(schema=1,version='2.0.0',build=build,files=files)))
        return path

    def test_same_version_new_content_has_new_build_identity(self):
        with tempfile.TemporaryDirectory(prefix='qa-app-release-',dir=ROOT/'target') as temporary:
            directory=Path(temporary)
            first=base.application_manifest(self.bundle(directory,'old'),'https://example.invalid/app.zip')
            second=base.application_manifest(self.bundle(directory,'new'),'https://example.invalid/app.zip')
            self.assertEqual(first['version'],second['version'])
            self.assertNotEqual(first['build'],second['build'])

    def test_missing_helper_damaged_files_and_http_are_rejected(self):
        with tempfile.TemporaryDirectory(prefix='qa-app-release-',dir=ROOT/'target') as temporary:
            directory=Path(temporary)
            for marker,kwargs in [('missing',dict(missing=True)),('damaged',dict(corrupt=True))]:
                with self.assertRaises(ValueError):
                    base.application_manifest(self.bundle(directory,marker,**kwargs),'https://example.invalid/app.zip')
            with self.assertRaises(ValueError):
                base.application_manifest(self.bundle(directory,'valid'),'http://example.invalid/app.zip')


if __name__ == '__main__':
    unittest.main()
