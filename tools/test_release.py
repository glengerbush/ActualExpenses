"""Exercise the CLI against real temporary Git history and a fake GitHub transport."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("release.py").resolve()
FAKE_GH = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
p = pathlib.Path(os.environ['FAKE_GH_STATE'])
s = json.loads(p.read_text())
a = sys.argv[1:]
def save(): p.write_text(json.dumps(s))
if a[0] == 'api':
    if '--paginate' in a:
        print(json.dumps([[], [s['release']] if s.get('release') else []])); sys.exit(0)
    if '/git/ref/' in a[1]:
        result = {'object': {'type': 'commit', 'sha': s['tag']}} if s.get('tag') else None
    else:
        result = s.get('release') if not s.get('release', {}).get('draft') else None
    if result is None:
        print('gh: Not Found (HTTP 404)', file=sys.stderr); sys.exit(1)
    print(json.dumps(result))
else:
    s.setdefault('commands', []).append(a)
    if a[1] == 'create':
        s['release'] = {'draft': True, 'tag_name': a[2], 'target_commitish': a[a.index('--target')+1], 'assets': []}
    elif a[1] == 'upload':
        if s.pop('fail_upload', False):
            save(); print('Upload interrupted', file=sys.stderr); sys.exit(1)
        s['release']['assets'] = [{'name': pathlib.Path(x).name} for x in a[3:a.index('--repo')]]
    elif a[1] == 'edit':
        s['release']['draft'] = False
        s['tag'] = s['release']['target_commitish']
    save()
'''


class ReleaseCliTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.email", "release-test@example.invalid")
        self.git("config", "user.name", "Release test")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True).strip()

    def commit(self, name=None, code=None):
        if name is not None:
            path = self.root / "app/build.gradle.kts"
            path.parent.mkdir(exist_ok=True)
            path.write_text(f'versionName = "{name}"\nversionCode = {code}\n')
            self.git("add", ".")
        self.git("commit", "--allow-empty", "-qm", "test")
        return self.git("rev-parse", "HEAD")

    def cli(self, *args, env=None, succeeds=True):
        result = subprocess.run([sys.executable, str(SCRIPT), *args], cwd=self.root,
                                env=env, capture_output=True, text=True)
        if succeeds:
            self.assertEqual(0, result.returncode, result.stderr)
        else:
            self.assertNotEqual(0, result.returncode)
        return result

    def test_ordinary_commit_skips_release(self):
        before = self.commit("0.1.0", 1)
        self.commit()
        self.assertEqual("false", json.loads(self.cli("plan", "--before", before).stdout)["release"])

    def test_bump_across_multiple_commits_emits_outputs(self):
        before = self.commit("0.9.0", 1)
        self.commit("0.10.0", 2)
        self.commit()
        output = self.root / "output"
        self.cli("plan", "--before", before, env=dict(os.environ, GITHUB_OUTPUT=str(output)))
        self.assertEqual("release=true\nversion=0.10.0\ntag=v0.10.0\n", output.read_text())

    def test_partial_or_backward_bumps_fail(self):
        before = self.commit("1.2.0", 5)
        for name, code in [("1.2.1", 5), ("1.2.0", 6), ("1.1.0", 6), ("1.3.0", 4)]:
            with self.subTest(name=name, code=code):
                self.commit(name, code)
                self.assertIn("BOTH", self.cli("plan", "--before", before, succeeds=False).stderr)

    def test_first_android_version_releases(self):
        before = self.commit()
        self.commit("0.1.0", 1)
        self.assertEqual("true", json.loads(self.cli("plan", "--before", before).stdout)["release"])

    def test_manual_dispatch_and_initial_push(self):
        self.commit("0.1.0", 1)
        for before in ["", "0" * 40]:
            self.assertEqual("true", json.loads(self.cli("plan", "--before", before).stdout)["release"])

    def test_unknown_base_fails_instead_of_releasing(self):
        self.commit("0.1.0", 1)
        self.cli("plan", "--before", "f" * 40, succeeds=False)

    def test_invalid_versions_fail(self):
        for name, code in [("01.2.3", 1), ("1.2", 1), ("1.2.3-beta", 1), ("1.2.3", 0)]:
            with self.subTest(name=name, code=code):
                self.commit(name, code)
                self.cli("plan", succeeds=False)

    def transport(self, state):
        binary = self.root / "gh"
        binary.write_text(FAKE_GH)
        binary.chmod(0o700)
        path = self.root / "github.json"
        path.write_text(json.dumps(state))
        env = dict(os.environ, FAKE_GH_STATE=str(path), PATH=f"{self.root}:{os.environ['PATH']}")
        directory = self.root / "dist"
        directory.mkdir()
        (directory / "ActualExpenses-0.1.0.apk").write_bytes(b"test artifact")
        (directory / "SHA256SUMS").write_text("test checksum")
        return path, env

    def test_interrupted_upload_resumes_draft_and_published_retry_is_noop(self):
        sha = self.commit("0.1.0", 1)
        path, env = self.transport({"fail_upload": True})
        command = ("publish", "--repository", "owner/repo", "--sha", sha)
        self.cli(*command, env=env, succeeds=False)
        self.assertTrue(json.loads(path.read_text())["release"]["draft"])
        self.cli(*command, env=env)
        state = json.loads(path.read_text())
        self.assertFalse(state["release"]["draft"])
        self.assertEqual(1, sum(c[1] == "create" for c in state["commands"]))
        self.assertEqual({"ActualExpenses-0.1.0.apk", "SHA256SUMS"}, {a["name"] for a in state["release"]["assets"]})
        self.cli(*command, env=env)
        self.assertEqual(state, json.loads(path.read_text()))

    def test_existing_tag_or_draft_for_other_commit_is_not_overwritten(self):
        sha = self.commit("0.1.0", 1)
        path, env = self.transport({"tag": "f" * 40})
        command = ("publish", "--repository", "owner/repo", "--sha", sha)
        self.cli(*command, env=env, succeeds=False)
        self.assertNotIn("commands", json.loads(path.read_text()))
        state = {"release": {"draft": True, "tag_name": "v0.1.0", "target_commitish": "f" * 40}}
        path.write_text(json.dumps(state))
        self.cli(*command, env=env, succeeds=False)
        self.assertEqual(state, json.loads(path.read_text()))


if __name__ == "__main__":
    unittest.main()
