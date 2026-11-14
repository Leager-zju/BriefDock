"""Package only the Windows host and CLI; optional service runtimes stay separate."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def notices(destination, names=('lazybuddy', 'lazybuddy-cli')):
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--offline', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc'], cwd=ROOT))
    packages = {package['id']: package for package in metadata['packages']}
    nodes = {node['id']: node for node in metadata['resolve']['nodes']}
    pending = [key for key, package in packages.items() if package['name'] in names]
    included = set()
    while pending:
        identifier = pending.pop()
        if identifier in included:
            continue
        included.add(identifier)
        pending.extend(nodes[identifier]['dependencies'])
    records = []
    for identifier in sorted(included):
        package = packages[identifier]
        if package['source'] is None and Path(package['manifest_path']).is_relative_to(ROOT / 'apps'):
            continue
        source = Path(package['manifest_path']).parent
        files = [path for path in source.iterdir() if path.is_file() and path.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE', 'COPYRIGHT'))]
        if files:
            directory = destination / f'{package["name"]}-{package["version"]}'
            directory.mkdir(parents=True)
            for path in files:
                shutil.copyfile(path, directory / path.name)
        records.append(dict(name=package['name'], version=package['version'], license=package['license'], repository=package['repository']))
    shutil.copytree(ROOT / 'assets/button-icons', destination / 'fluent-icons', ignore=shutil.ignore_patterns('*.svg', '*.png', '*.bin'))
    (destination / 'dependencies.json').write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--profile', choices=('debug', 'release'), default='release')
    parser.add_argument('--catalog-source', type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / 'target') or output.exists():
        raise ValueError('new output directory inside target required')
    directory = output / 'LazyBuddy'
    directory.mkdir(parents=True)
    for filename in ('lazybuddy.exe', 'lazybuddy-cli.exe'):
        shutil.copyfile(ROOT / 'target' / args.profile / filename, directory / filename)
    if args.catalog_source:
        source = json.loads(args.catalog_source.read_text(encoding='utf-8'))
        if set(source) != {'url', 'public_key'} or not source['url'].startswith('https://') or len(source['public_key']) != 32 or any(type(value) is not int or not 0 <= value <= 255 for value in source['public_key']):
            raise ValueError('invalid trusted catalog source')
        shutil.copyfile(args.catalog_source, directory / 'module-catalog.json')
    (directory / 'README.txt').write_text(
        'LazyBuddy · Windows x64\n\n'
        '运行 lazybuddy.exe 打开桌面搭子。基础包只包含主程序与 CLI，功能需要单独安装。\n'
        '附有正式可信目录配置时，可在「功能」中下载安装或卸载新闻、短剧。\n'
        '当前开发包尚未连接公开目录；开发者可在退出宿主后使用：\n'
        'lazybuddy-cli modules install-local <功能包目录>\n\n'
        '显示、系统和关于设置为全局设置；当前功能的账号和偏好单独保存。\n'
        '用户数据位于 %LOCALAPPDATA%\\LazyBuddy，账号凭据由 Windows 安全存储管理。\n'
        '第三方许可见 licenses；开发分发状态见 NOTICE.txt（若存在）。\n', encoding='utf-8')
    if (ROOT / 'LICENSE').is_file():
        shutil.copyfile(ROOT / 'LICENSE', directory / 'LICENSE')
    else:
        (directory / 'NOTICE.txt').write_text('Development build. Project license and public distribution are not yet approved.\n', encoding='utf-8')
    notices(directory / 'licenses')
    archive = output / 'LazyBuddy-windows-x86_64.zip'
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as package:
        for path in sorted(directory.rglob('*')):
            if path.is_file():
                package.write(path, path.relative_to(output).as_posix())
    with archive.open('rb') as file:
        digest = hashlib.file_digest(file, 'sha256').hexdigest()
    result = dict(archive=str(archive), bytes=archive.stat().st_size, sha256=digest, optional_modules_bundled=False)
    (output / 'package.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
