"""Run only unit tests owned by changed code; GUI/integration suites are explicit."""
import argparse
import json
from pathlib import Path
import subprocess
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
    if checks:
        commands.append(['cargo', 'check', '--locked', *[arg for package in sorted(checks) for arg in ('-p', package)]])
    for package, module in sorted(units):
        commands.append(['cargo', 'test', '--locked', '-p', package, *([module] if module else [])])
    for name in sorted(python_tests):
        commands.append(['python', '-B', name])
    return commands


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base')
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()
    if not args.base:
        args.base = 'HEAD^'
    if set(args.base) != {'0'}:
        paths = subprocess.check_output(['git', 'diff', '--name-only', args.base, 'HEAD'], cwd=ROOT, text=True).splitlines()
    else:
        paths = subprocess.check_output(['git', 'ls-files'], cwd=ROOT, text=True).splitlines()
    commands = plan(paths)
    for command in commands:
        print(json.dumps(command), flush=True)
        if not args.dry_run:
            subprocess.run(command, cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
