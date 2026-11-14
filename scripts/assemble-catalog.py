"""Assemble release ZIP metadata without importing or building service source."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile


def package_entry(path, base_url):
    with zipfile.ZipFile(path) as archive:
        info=archive.getinfo('module.json')
        if info.file_size>65536:raise ValueError('manifest too large')
        manifest=json.loads(archive.read(info))
        if manifest.get('schema')!=1 or manifest.get('service_api')!=1:raise ValueError('unsupported package API')
        if not re.fullmatch('[a-z][a-z0-9-]{0,63}',manifest['id']):raise ValueError('invalid package ID')
        if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)',manifest['version']):raise ValueError('invalid package version')
        entrypoint=manifest['entrypoint']
        if not entrypoint or any(part in ('','..','.') for part in entrypoint.split('/')) or any(char in entrypoint for char in '\\:'):raise ValueError('invalid package entrypoint')
        archive.getinfo(entrypoint)
    with path.open('rb') as source:digest=hashlib.file_digest(source,'sha256').hexdigest()
    return dict(id=manifest['id'],name=manifest['name'],kind=manifest.get('kind','feature'),version=manifest['version'],target=manifest['target'],dependencies=manifest.get('dependencies',[]),url=base_url.rstrip('/')+'/'+path.name,sha256=digest,bytes=path.stat().st_size)


def assemble(paths,base_url):
    if not base_url.startswith('https://'):raise ValueError('HTTPS base URL required')
    entries=[package_entry(path,base_url) for path in paths]
    by_id={entry['id']:entry for entry in entries}
    if len(by_id)!=len(entries):raise ValueError('duplicate package ID')
    for entry in entries:
        for dependency in entry['dependencies']:
            if dependency['id'] not in by_id or by_id[dependency['id']]['version']!=dependency['version']:raise ValueError('missing exact runtime dependency')
    return dict(schema=1,packages=entries)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packages',type=Path,nargs='+',required=True)
    parser.add_argument('--base-url',required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    result=assemble(args.packages,args.base_url)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x',encoding='utf-8') as file:json.dump(result,file,ensure_ascii=False,indent=2)
    print(json.dumps(dict(packages=len(result['packages']))))


if __name__=='__main__':main()
