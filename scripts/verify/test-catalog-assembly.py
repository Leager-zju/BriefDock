"""Catalog assembly rejects incomplete/ambiguous packages before signing."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('catalog',ROOT/'scripts/assemble-catalog.py')
catalog=importlib.util.module_from_spec(spec);spec.loader.exec_module(catalog)


class Catalog(unittest.TestCase):
    def test_complete_dependency_set_and_rejections(self):
        with tempfile.TemporaryDirectory(prefix='qa-catalog-',dir=ROOT/'target') as temporary:
            def package(identifier,dependencies):
                path=Path(temporary)/(identifier+'.zip')
                manifest=dict(schema=1,service_api=1,id=identifier,name=identifier,version='0.1.0',target='windows-x86_64',entrypoint='bin/service.exe',dependencies=dependencies,kind='runtime' if identifier=='runtime' else 'feature',page=None if identifier=='runtime' else dict(template='news_grid',catalog_method='sources',fetch_method='fetch'))
                with zipfile.ZipFile(path,'w') as archive:
                    archive.writestr('module.json',json.dumps(manifest));archive.writestr('bin/service.exe',b'fixture')
                return path
            runtime=package('runtime',[])
            feature=package('feature',[dict(id='runtime',version='0.1.0')])
            self.assertEqual(len(catalog.assemble([runtime,feature],'https://example.invalid')['packages']),2)
            for paths,url in [([feature],'https://example.invalid'),([runtime,runtime],'https://example.invalid'),([runtime],'http://example.invalid')]:
                with self.assertRaises(ValueError):catalog.assemble(paths,url)


if __name__=='__main__':unittest.main()
