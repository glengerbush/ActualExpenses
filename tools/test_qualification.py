import collections
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

import qualification as q


class QualificationTests(unittest.TestCase):
    def test_every_browser_check_runs_exactly_once_across_shards(self):
        spec = importlib.util.spec_from_file_location('container_smoke', q.ROOT / 'actualBudget/tools/container-smoke.py')
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        commands = [{'cwd': 'web', 'argv': c} for c in smoke.browser_commands(q.ROOT / 'actualBudget')]
        self.assertGreater(len(commands), 100)
        self.assertIn(['node', 'tests/import-storage-browser.mjs'], [c['argv'] for c in commands])
        for kind in ('transaction', 'action'):
            self.assertIn(['node', 'tests/concurrency-browser.mjs', f'--recovery-kind={kind}'],
                          [c['argv'] for c in commands])
        self.assert_partition(commands, 8)

    def test_every_source_check_runs_exactly_once_including_expanded_npm_commands(self):
        commands = q.source_commands()
        self.assertGreater(len(commands), 60)
        self.assertIn(['python3', 'actualBudget/tools/generate-v5-receipts.py', '--check'], [c['argv'] for c in commands])
        self.assert_partition(commands, 4)

    def assert_partition(self, commands, count):
        key = lambda c: (c['cwd'], tuple(c['argv']))
        groups = [q.select_shard(commands, i, count) for i in range(count)]
        self.assertTrue(all(groups))
        self.assertEqual(collections.Counter(map(key, commands)),
                         collections.Counter(key(c) for group in groups for c in group))
        self.assertEqual(groups, [q.select_shard(commands, i, count) for i in range(count)])

    def test_invalid_shard_cannot_silently_skip_checks(self):
        for index, count in [(-1, 2), (2, 2), (0, 0), (0, 3)]:
            with self.assertRaises(ValueError):
                q.select_shard([{'cwd': '.', 'argv': ['one']}, {'cwd': '.', 'argv': ['two']}], index, count)

    def test_failed_command_still_runs_remaining_checks_and_fails_group(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = [
                {'cwd': '.', 'argv': [sys.executable, '-c', 'raise SystemExit(7)']},
                {'cwd': '.', 'argv': [sys.executable, '-c', 'from pathlib import Path; Path("ran").touch()']},
            ]
            with self.assertRaises(RuntimeError):
                q.run_commands(commands, root, os.environ.copy(), root / 'results.json')
            self.assertTrue((root / 'ran').is_file())
            results = json.loads((root / 'results.json').read_text())
            self.assertEqual([r['exit_code'] for r in results], [7, 0])
            self.assertTrue(all(r['seconds'] >= 0 for r in results))


if __name__ == '__main__':
    unittest.main()
