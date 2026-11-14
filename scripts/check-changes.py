"""Run only unit tests owned by changed code; GUI/integration suites are explicit."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def plan(paths, root=ROOT):
    workspace = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))['workspace']
    members = {member: tomllib.loads((root / member / 'Cargo.toml').read_text(encoding='utf-8'))['package']['name']
               for member in workspace['members']}
    units = set()
    checks = set()
    python_tests = set()
    for name in paths:
        path = Path(name)
        if name.startswith('scripts/verify/test-') and path.suffix == '.py':
            # UI/network integration scripts are never selected by the unit runner.
            if name in ('scripts/verify/test-select-tests.py', 'scripts/verify/test-app-release.py', 'scripts/verify/test-catalog-assembly.py', 'scripts/verify/test-signing-key.py'):
                python_tests.add(name)
        if name in ('scripts/check-changes.py', '.github/workflows/checks.yml'):
            python_tests.add('scripts/verify/test-select-tests.py')
        if name in ('scripts/package_base.py', 'scripts/create-app-update.py', 'scripts/sign-update.py'):
            python_tests.add('scripts/verify/test-app-release.py')
        if name.startswith('docs/updates/') or name == 'app-update-source.json':
            python_tests.add('scripts/verify/test-app-release.py')
        if name in ('scripts/assemble-catalog.py',):
            python_tests.add('scripts/verify/test-catalog-assembly.py')
        if name.startswith('docs/modules/') or name in ('scripts/sign-catalog.py', 'scripts/windows_signing_key.py', '.gitattributes', 'module-catalog.json'):
            python_tests.add('scripts/verify/test-signing-key.py')
        for member, package in members.items():
            if not name.startswith(member + '/'):
                continue
            checks.add(package)
            relative = path.relative_to(member).as_posix()
            source = root / name
            if not relative.startswith('src/') or not source.is_file():
                continue
            content = source.read_text(encoding='utf-8')
            if '#[test]' in content or 'mod tests;' in content:
                module = relative.removeprefix('src/').removesuffix('.rs').replace('/', '::')
                if module in ('lib', 'main'):
                    module = 'tests' if 'mod tests;' in content else ''
                elif module != 'tests' and not module.endswith('::tests'):
                    module += '::tests'
                units.add((package, module))
            elif (root / member / 'src/tests.rs').is_file():
                units.add((package, 'tests'))
    if 'Cargo.lock' in paths or 'Cargo.toml' in paths:
        checks.update(members.values())  # Compile interfaces; do not run unrelated tests.
    commands = []
    composition = any(name.startswith('crates/media-ipc/') for name in paths)
    if checks:
        commands.append(['cargo', 'check', '--locked', *[arg for package in sorted(checks) for arg in ('-p', package)],
                         *(['--features', 'lazybuddy-media-ipc/composition'] if composition else [])])
    for package, module in sorted(units):
        commands.append(['cargo', 'test', '--locked', '-p', package,
                         *(['--features', 'composition'] if composition and package == 'lazybuddy-media-ipc' else []),
                         *([module] if module else [])])
    for name in sorted(python_tests):
        commands.append([sys.executable, '-B', name])
    return commands


def changed_paths(base, root=ROOT):
    if not base or set(base) == {'0'}:
        # A new branch has an all-zero event.before, not a repository-wide change.
        common = subprocess.run(['git', 'merge-base', 'HEAD', 'origin/main'], cwd=root,
                                text=True, capture_output=True)
        head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        if common.returncode == 0 and common.stdout.strip() != head:
            base = common.stdout.strip()
        elif subprocess.run(['git', 'rev-parse', '--verify', 'HEAD^'], cwd=root,
                            capture_output=True).returncode == 0:
            base = 'HEAD^'
        else:
            return subprocess.check_output(['git', 'diff-tree', '--root', '--no-commit-id',
                                            '--name-only', '-r', 'HEAD'], cwd=root, text=True).splitlines()
    return subprocess.check_output(['git', 'diff', '--name-only', base, 'HEAD'], cwd=root, text=True).splitlines()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base')
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()
    commands = plan(changed_paths(args.base))
    for command in commands:
        print(json.dumps(command), flush=True)
        if not args.dry_run:
            subprocess.run(command, cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
