"""Unit selection must not silently expand a feature change into a full suite."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('selection', ROOT/'scripts/check-changes.py')
selection = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selection)


class Selection(unittest.TestCase):
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
