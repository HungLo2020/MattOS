"""An ignored upstream file must not disappear silently from a checkout."""
import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('tracking', ROOT / 'DevUtils/audits/test_vendored_source_tracking.py')
tracking = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tracking)


class ImportCompletenessTests(unittest.TestCase):
    def test_ignored_missing_file_is_detected_and_unstaged_repair_is_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            source = root / 'src/example'
            source.mkdir(parents=True)
            (source / '.gitignore').write_text('hidden.txt\n')
            (root / 'upstream/state').mkdir(parents=True)
            (root / 'upstream/sources.toml').write_text('[[component]]\nname="example"\npath="src/example"\n')
            subprocess.run(['git', 'add', 'src/example/.gitignore'], cwd=root, check=True)
            records = []
            for name, data in [('.gitignore', b'hidden.txt\n'), ('hidden.txt', b'upstream\n')]:
                oid = hashlib.sha1(f'blob {len(data)}\0'.encode() + data).hexdigest()
                records.append(f'100644 blob {oid}\t{name}\0'.encode())
            digest = hashlib.sha256(b''.join(records)).hexdigest()
            (root / 'upstream/state/example.toml').write_text(f'imported_tree_digest="{digest}"\n')
            self.assertEqual(tracking.incomplete_imports(root), ['example'])
            self.assertEqual(tracking.incomplete_imports(root, worktree=True), ['example'])
            (source / 'hidden.txt').write_bytes(b'upstream\n')
            self.assertEqual(tracking.incomplete_imports(root), ['example'])
            self.assertEqual(tracking.incomplete_imports(root, worktree=True), [])
            (source / 'hidden.txt').write_bytes(b'wrong bytes\n')
            self.assertEqual(tracking.incomplete_imports(root, worktree=True), ['example'])


if __name__ == '__main__':
    unittest.main()
