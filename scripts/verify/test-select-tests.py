"""Unit selection must not silently expand a feature change into a full suite."""
import importlib.util
from pathlib import Path
import unittest
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('selection', ROOT/'scripts/check-changes.py')
selection = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selection)


class Selection(unittest.TestCase):
    def test_new_branch_zero_before_compares_main_instead_of_all_files(self):
        with tempfile.TemporaryDirectory(prefix='qa-selection-') as temporary:
            root = Path(temporary)
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=root, text=True, stderr=subprocess.DEVNULL)
            git('init', '--initial-branch=main')
            (root/'existing.rs').write_text('baseline', encoding='utf-8')
            git('add', '.')
            git('-c', 'user.name=Unit', '-c', 'user.email=unit@example.invalid', 'commit', '-m', 'baseline')
            git('update-ref', 'refs/remotes/origin/main', 'HEAD')
            git('switch', '-c', 'feature')
            (root/'prototype.ts').write_text('changed', encoding='utf-8')
            git('add', '.')
            git('-c', 'user.name=Unit', '-c', 'user.email=unit@example.invalid', 'commit', '-m', 'feature')
            self.assertEqual(selection.changed_paths('0'*40, root), ['prototype.ts'])

    def test_updater_changes_select_own_unit_tests_only(self):
        commands = selection.plan(['apps/desktop/src/main.rs', 'apps/desktop/src/updater.rs', 'crates/app-updater/src/tests.rs'])
        tests = [command for command in commands if command[:2] == ['cargo', 'test']]
        self.assertIn(['cargo', 'test', '--locked', '-p', 'lazybuddy', 'updater::tests'], tests)
        self.assertIn(['cargo', 'test', '--locked', '-p', 'lazybuddy-app-updater', 'tests'], tests)
        self.assertTrue(all('--workspace' not in command for command in commands))
        self.assertTrue(all('test-shell-ui.py' not in str(command) for command in commands))

    def test_dependency_changes_compile_without_forcing_all_tests(self):
        commands = selection.plan(['Cargo.lock'])
        self.assertEqual(len(commands), 1)
        self.assertEqual(commands[0][:2], ['cargo', 'check'])

    def test_documents_do_not_trigger_tests(self):
        self.assertEqual(selection.plan(['README.md', 'TASKS.md']), [])


if __name__ == '__main__':
    unittest.main()
