"""Exercise the completion gate without changing the real repository or index."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class CompletionGateTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git('init')
        self.hook = self.root / '.agents/hooks/require_commit.py'
        self.hook.parent.mkdir(parents=True)
        shutil.copyfile(Path(__file__).with_name('require_commit.py'), self.hook)
        (self.root / '.gitignore').write_text('.agents/\nignored.txt\n')
        self.git('add', '.gitignore')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root)

    def run_hook(self, active=False):
        before = self.git('status', '--porcelain')
        result = subprocess.run(
            [sys.executable, str(self.hook)], cwd=self.root,
            input=json.dumps({'stop_hook_active': active}), text=True,
            capture_output=True, check=True,
        )
        self.assertEqual(before, self.git('status', '--porcelain'))
        return json.loads(result.stdout)

    def test_staged_changes_block(self):
        self.assertEqual(self.run_hook()['decision'], 'block')

    def test_untracked_changes_block(self):
        self.git('rm', '--cached', '.gitignore')
        (self.root / '.gitignore').unlink()
        (self.root / 'new.txt').write_text('new')
        self.assertEqual(self.run_hook()['decision'], 'block')

    def test_repeated_failure_stops_with_reason(self):
        result = self.run_hook(active=True)
        self.assertFalse(result['continue'])
        self.assertIn('Completion check failed', result['stopReason'])

    def test_clean_tree_and_ignored_files_pass(self):
        # An empty index and only ignored working files are clean, even before HEAD exists.
        self.git('rm', '--cached', '.gitignore')
        (self.root / '.git/info/exclude').write_text('.agents/\n.gitignore\nignored.txt\n')
        (self.root / 'ignored.txt').write_text('generated')
        self.assertEqual(self.run_hook(), {})
        result = subprocess.run([sys.executable, str(self.hook), '--check'],
                                cwd=self.root, capture_output=True)
        self.assertEqual(result.returncode, 0)

    def test_manual_check_fails_when_dirty(self):
        result = subprocess.run([sys.executable, str(self.hook), '--check'],
                                cwd=self.root, capture_output=True)
        self.assertEqual(result.returncode, 1)


if __name__ == '__main__':
    unittest.main()
