"""Release CLI contracts exercised with real temporary Git histories."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("release.py").resolve()
PUBLISH = Path(__file__).with_name("publish.py").resolve()


class ReleasePlanTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Release test")
        self.write("release-policy.json", json.dumps({"actualExpenses": True, "actualBudget": False}))
        self.android("0.1.2", 3)
        self.budget("0.1.0")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True).strip()

    def write(self, path, contents):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(contents)

    def android(self, version, code):
        self.write("actualExpenses/app/build.gradle.kts", f'versionName = "{version}"\nversionCode = {code}\n')

    def budget(self, version):
        self.write("actualBudget/VERSION", version + "\n")
        self.write("actualBudget/server/Cargo.toml", f'[package]\nname="actualbudget-server"\nversion="{version}"\n')
        self.write("actualBudget/server/Cargo.lock", f'[[package]]\nname="actualbudget-server"\nversion="{version}"\n')
        self.write("actualBudget/web/package.json", json.dumps({"version": version}))
        self.write("actualBudget/web/package-lock.json", json.dumps({"version": version, "packages": {"": {"version": version}}}))

    def commit(self):
        self.git("add", ".")
        self.git("commit", "--allow-empty", "-qm", "test")
        return self.git("rev-parse", "HEAD")

    def cli(self, *args, succeeds=True, env=None):
        result = subprocess.run([sys.executable, str(SCRIPT), *args], cwd=self.root,
                                env=env, text=True, capture_output=True)
        self.assertEqual(result.returncode == 0, succeeds, result.stdout + result.stderr)
        return result

    def test_android_bump_releases_only_android_in_new_namespace(self):
        before = self.commit()
        self.android("0.1.3", 4)
        self.commit()
        result = json.loads(self.cli("plan", "--before", before).stdout)
        self.assertTrue(result["apps"]["actualExpenses"]["publish"])
        self.assertEqual("actualExpenses-v0.1.3", result["apps"]["actualExpenses"]["tag"])
        self.assertFalse(result["apps"]["actualBudget"]["changed"])

    def test_budget_bump_stays_blocked_until_final_acceptance(self):
        before = self.commit()
        self.budget("0.2.0")
        self.commit()
        result = json.loads(self.cli("plan", "--before", before).stdout)
        self.assertFalse(result["apps"]["actualExpenses"]["publish"])
        self.assertTrue(result["apps"]["actualBudget"]["changed"])
        self.assertFalse(result["apps"]["actualBudget"]["publish"])
        self.assertIn("final acceptance", result["apps"]["actualBudget"]["blocked_reason"])
        self.write("release-policy.json", json.dumps({"actualExpenses": True, "actualBudget": True}))
        self.commit()
        result = json.loads(self.cli("plan", "--before", before).stdout)
        self.assertTrue(result["apps"]["actualBudget"]["publish"])
        self.assertEqual("latest", result["apps"]["actualBudget"]["moving_alias"])

    def test_unchanged_candidate_is_explicitly_separate_from_stable(self):
        before = self.commit()
        result = json.loads(self.cli("plan", "--before", before, "--channel", "candidate", "--app", "actualBudget").stdout)
        candidate = result["apps"]["actualBudget"]
        self.assertTrue(candidate["publish"])
        self.assertIsNone(candidate["moving_alias"])
        self.assertEqual(f"actualBudget-v0.1.0-candidate.{before[:12]}", candidate["tag"])
        self.assertEqual(f"0.1.0-candidate.{before[:12]}", candidate["image_tag"])
        self.assertFalse(result["apps"]["actualExpenses"]["publish"])
        unselected = json.loads(self.cli("plan", "--before", before, "--channel", "candidate").stdout)
        self.assertTrue(all(not app["publish"] for app in unselected["apps"].values()))

    def test_metadata_introduction_and_ordinary_commits_do_not_release(self):
        (self.root / "actualBudget/VERSION").unlink()
        before = self.commit()
        self.write("actualBudget/VERSION", "0.1.0\n")
        self.commit()
        self.commit()
        result = json.loads(self.cli("plan", "--before", before).stdout)
        self.assertTrue(all(not app["changed"] for app in result["apps"].values()))

    def test_complete_push_and_github_outputs_keep_both_independent(self):
        before = self.commit()
        self.android("0.1.3", 4)
        self.commit()
        self.budget("0.2.0")
        self.commit()
        self.commit()
        outputs = self.root / "outputs"
        result = json.loads(self.cli("plan", "--before", before, env=dict(os.environ, GITHUB_OUTPUT=str(outputs))).stdout)
        self.assertTrue(all(app["changed"] for app in result["apps"].values()))
        self.assertIn("android_publish=true\n", outputs.read_text())
        self.assertIn("budget_publish=false\n", outputs.read_text())

    def test_partial_android_bump_and_divergent_web_version_fail(self):
        before = self.commit()
        self.android("0.1.3", 3)
        self.commit()
        self.assertIn("BOTH", self.cli("plan", "--before", before, succeeds=False).stderr)
        self.android("0.1.3", 4)
        self.write("actualBudget/web/package.json", '{"version":"0.2.0"}')
        self.commit()
        self.assertIn("share one version", self.cli("plan", "--before", before, succeeds=False).stderr)

    def test_missing_unknown_and_nonancestor_bases_fail_closed(self):
        before = self.commit()
        for value in ("", "0" * 40, "f" * 40):
            self.cli("plan", "--before", value, succeeds=False)
        self.git("checkout", "--orphan", "unrelated")
        self.write("unrelated.txt", "Different root history\n")
        unrelated = self.commit()
        self.cli("plan", "--before", before, "--head", unrelated, succeeds=False)

    def test_invalid_version_and_policy_are_rejected(self):
        before = self.commit()
        self.budget("01.2.3")
        self.commit()
        self.cli("plan", "--before", before, succeeds=False)
        self.budget("0.1.0")
        self.write("release-policy.json", '{"actualExpenses":true,"actualBudget":"false"}')
        self.commit()
        self.cli("plan", "--before", before, succeeds=False)

    def test_publish_dry_run_has_no_remote_effects_and_gate_blocks_execution(self):
        before = self.commit()
        command = [sys.executable, str(PUBLISH), "actualBudget", "--before", before]
        dry = subprocess.run(command, cwd=self.root, text=True, capture_output=True)
        self.assertEqual(0, dry.returncode, dry.stderr)
        self.assertFalse(json.loads(dry.stdout)["would_publish"])
        denied = subprocess.run([*command, "--execute"], cwd=self.root, text=True, capture_output=True)
        self.assertNotEqual(0, denied.returncode)
        self.assertIn("final acceptance", denied.stderr)

    def test_artifact_checksum_tampering_and_unexpected_paths_fail(self):
        self.write("artifacts/image.json", '{"digest":"example"}')
        command = [sys.executable, str(SCRIPT.with_name("artifacts.py")), "checksums",
                   "--directory", str(self.root / "artifacts"), "image.json"]
        generated = subprocess.run([*command, "--write"], capture_output=True, text=True)
        self.assertEqual(0, generated.returncode, generated.stderr)
        self.assertEqual(0, subprocess.run(command, capture_output=True).returncode)
        self.write("artifacts/image.json", '{"digest":"changed"}')
        self.assertNotEqual(0, subprocess.run(command, capture_output=True).returncode)
        self.write("artifacts/SHA256SUMS", "a" * 64 + "  ../image.json\n")
        self.assertNotEqual(0, subprocess.run(command, capture_output=True).returncode)

    def transport(self, state):
        binaries = self.root / "bin"
        binaries.mkdir()
        script = SCRIPT.with_name("fixtures") / "release_transport.py"
        for name in ("docker", "gh", "cosign", "apksigner", "aapt"):
            target = binaries / name
            target.write_text(script.read_text())
            target.chmod(0o755)
        path = self.root / "transport.json"
        path.write_text(json.dumps(state))
        return path, dict(os.environ, RELEASE_TRANSPORT_STATE=str(path), PATH=f"{binaries}:{os.environ['PATH']}")

    def test_candidate_publication_signs_before_version_tag_and_never_moves_latest(self):
        commit = self.commit()
        path, env = self.transport({"commit": commit, "version": "0.1.0"})
        command = [sys.executable, str(PUBLISH), "actualBudget", "--before", commit,
                   "--channel", "candidate", "--image", "local:test", "--directory", str(self.root / "dist")]
        dry = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
        self.assertEqual(0, dry.returncode, dry.stderr)
        self.assertTrue(json.loads(dry.stdout)["would_publish"])
        self.assertNotIn("events", json.loads(path.read_text()))
        published = subprocess.run([*command, "--execute"], cwd=self.root, env=env, text=True, capture_output=True)
        self.assertEqual(0, published.returncode, published.stderr)
        state = json.loads(path.read_text())
        self.assertTrue(state["release"]["prerelease"])
        self.assertFalse(any(ref.endswith(":latest") for ref in state["images"]))
        actions = state["events"]
        signed = next(i for i, event in enumerate(actions) if event[:2] == ["cosign", "verify"])
        promoted = next(i for i, event in enumerate(actions) if event[:4] == ["docker", "buildx", "imagetools", "create"])
        self.assertLess(signed, promoted)

    def test_interrupted_image_release_resumes_without_replacing_immutable_image(self):
        commit = self.commit()
        path, env = self.transport({"commit": commit, "version": "0.1.0", "interrupt_upload": True})
        command = [sys.executable, str(PUBLISH), "actualBudget", "--before", commit,
                   "--channel", "candidate", "--image", "local:test", "--directory", str(self.root / "dist"), "--execute"]
        first = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
        self.assertNotEqual(0, first.returncode)
        self.assertTrue(json.loads(path.read_text())["release"]["draft"])
        for _ in range(2):
            resumed = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
            self.assertEqual(0, resumed.returncode, resumed.stderr)
        state = json.loads(path.read_text())
        self.assertFalse(state["release"]["draft"])
        self.assertEqual(1, sum(event[:2] == ["docker", "push"] for event in state["events"]))
        self.assertEqual(1, sum(event[:3] == ["gh", "release", "create"] for event in state["events"]))

    def test_signature_failure_never_creates_a_version_or_latest_tag(self):
        commit = self.commit()
        path, env = self.transport({"commit": commit, "version": "0.1.0", "reject_signature": True})
        command = [sys.executable, str(PUBLISH), "actualBudget", "--before", commit,
                   "--channel", "candidate", "--image", "local:test", "--execute"]
        result = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
        self.assertNotEqual(0, result.returncode)
        state = json.loads(path.read_text())
        self.assertTrue(all(":build-" in ref for ref in state["images"]))
        self.assertNotIn("release", state)

    def test_stable_alias_moves_only_after_release_and_never_backwards(self):
        before = self.commit()
        self.budget("0.2.0")
        self.write("release-policy.json", '{"actualExpenses":true,"actualBudget":true}')
        commit = self.commit()
        path, env = self.transport({"commit": commit, "version": "0.2.0"})
        command = [sys.executable, str(PUBLISH), "actualBudget", "--before", before,
                   "--image", "local:test", "--directory", str(self.root / "dist"), "--execute"]
        result = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
        self.assertEqual(0, result.returncode, result.stderr)
        state = json.loads(path.read_text())
        latest = "ghcr.io/glengerbush/actualbudget:latest"
        self.assertEqual(state["images"]["ghcr.io/glengerbush/actualbudget:0.2.0"], state["images"][latest])
        published = next(i for i, event in enumerate(state["events"]) if event[:3] == ["gh", "release", "edit"])
        moved = next(i for i, event in enumerate(state["events"]) if event[:4] == ["docker", "buildx", "imagetools", "create"] and latest in event)
        self.assertLess(published, moved)
        newer = "sha256:" + "b" * 64
        state["images"][latest] = newer
        state["signed"]["ghcr.io/glengerbush/actualbudget@" + newer] = {"version": "0.3.0", "git_sha": "f" * 40}
        path.write_text(json.dumps(state))
        retry = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
        self.assertEqual(0, retry.returncode, retry.stderr)
        self.assertEqual(newer, json.loads(path.read_text())["images"][latest])

    def test_existing_version_for_another_commit_is_never_replaced(self):
        before = self.commit()
        self.budget("0.2.0")
        self.write("release-policy.json", '{"actualExpenses":true,"actualBudget":true}')
        commit = self.commit()
        digest = "sha256:" + "b" * 64
        path, env = self.transport({"commit": commit, "version": "0.2.0",
            "images": {"ghcr.io/glengerbush/actualbudget:0.2.0": digest},
            "signed": {"ghcr.io/glengerbush/actualbudget@" + digest: {"version": "0.2.0", "git_sha": "f" * 40}}})
        result = subprocess.run([sys.executable, str(PUBLISH), "actualBudget", "--before", before, "--execute"],
                                cwd=self.root, env=env, text=True, capture_output=True)
        self.assertNotEqual(0, result.returncode)
        state = json.loads(path.read_text())
        self.assertNotIn("release", state)
        self.assertEqual({"ghcr.io/glengerbush/actualbudget:0.2.0": digest}, state["images"])

    def test_android_publication_uses_new_namespace_and_rejects_a_different_signer(self):
        before = self.commit()
        self.git("tag", "v0.1.2", before)
        self.android("0.1.3", 4)
        commit = self.commit()
        self.write("dist/ActualExpenses-0.1.3.apk", "External signing tool fixture")
        checksums = [sys.executable, str(SCRIPT.with_name("artifacts.py")), "checksums", "--directory",
                     str(self.root / "dist"), "--write", "ActualExpenses-0.1.3.apk"]
        subprocess.run(checksums, check=True)
        path, env = self.transport({"commit": commit, "version": "0.1.3", "code": 4, "apk_certificate": "f" * 64})
        command = [sys.executable, str(PUBLISH), "actualExpenses", "--before", before,
                   "--directory", str(self.root / "dist"), "--execute"]
        rejected = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
        self.assertNotEqual(0, rejected.returncode)
        self.assertIn("signing identity differs", rejected.stderr)
        state = json.loads(path.read_text())
        self.assertFalse(any(event[0] == "gh" for event in state["events"]))
        del state["apk_certificate"]
        path.write_text(json.dumps(state))
        published = subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)
        self.assertEqual(0, published.returncode, published.stderr)
        self.assertEqual("actualExpenses-v0.1.3", json.loads(path.read_text())["release"]["tag_name"])
        self.assertEqual(before, self.git("rev-parse", "v0.1.2"))


if __name__ == "__main__":
    unittest.main()
