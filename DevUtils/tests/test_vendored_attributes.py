"""The provenance audit must reject even ignored upstream attributes."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('provenance', ROOT / 'DevUtils/audits/test_vendored_source_provenance.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AttributesResidueTests(unittest.TestCase):
    def test_ignored_nested_attributes_fail_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            (root / '.gitignore').write_text('.gitattributes\n')
            source = root / 'src/example'
            source.mkdir(parents=True)
            (source / 'retained').write_bytes(b'upstream\n')
            subprocess.run(['git', 'add', 'src/example/retained'], cwd=root, check=True)
            entries = [('100644', 'blob', audit.git_blob_oid(b'upstream\n'), 'retained')]
            state = {'upstream_tree': 'tree', 'imported_tree_digest': audit.imported_digest(entries),
                     'imported_tree_digest_algorithm': audit.IMPORTED_DIGEST_ALGORITHM}
            component = {'name': 'example', 'path': 'src/example'}
            original_run = audit.run
            def fixture_run(command, **kwargs):
                kwargs['cwd'] = root
                return original_run(command, **kwargs)
            with mock.patch.object(audit, 'ROOT', root), mock.patch.object(audit, 'run', side_effect=fixture_run):
                self.assertEqual(audit.verify_component_tree(component, state, 'tree', entries, {}, None, None, {})[1], [])
                for name in ('.gitattributes', '3rdparty/.gitattributes', 'platforms/winpack/.gitattributes'):
                    path = source / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('* text eol=crlf\n')
                ignored, failures = audit.verify_component_tree(component, state, 'tree', entries, {}, None, None, {})
            self.assertEqual(ignored, 3)
            self.assertEqual(len(failures), 3, failures)
            self.assertTrue(all('forbidden upstream attributes' in failure for failure in failures))
