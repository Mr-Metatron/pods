"""Exercise build artifact selection and failure handling without a Rust SDK."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HELPER = Path(__file__).resolve().parents[1] / "build-aux" / "cargo-build.py"


class CargoBuildTests(unittest.TestCase):
    def run_build(self, directory, status=0, report_artifact=True, manifest=False):
        artifact = directory / "custom target" / "release" / "pods"
        artifact.parent.mkdir(parents=True)
        artifact.write_text("executable\n")
        artifact.chmod(0o755)
        command_log = directory / "arguments.json"
        fake_cargo = directory / "fake cargo"
        message = {
            "reason": "compiler-artifact",
            "target": {"name": "pods", "kind": ["bin"]},
            "executable": str(artifact),
        }
        fake_cargo.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\n"
            f"with open({str(command_log)!r}, 'w') as log:\n"
            "    json.dump({'args': sys.argv[1:], 'home': os.environ.get('CARGO_HOME'), 'cwd': os.getcwd()}, log)\n"
            + (
                f"print({json.dumps(message)!r})\n"
                if report_artifact
                else ""
            )
            + f"sys.exit({status})\n"
        )
        fake_cargo.chmod(0o755)
        output = directory / "meson output"
        env = dict(os.environ, CARGO_HOME=str(directory / "cargo cache"))
        extra_args = []
        if manifest:
            source_dir = directory / "source project"
            source_dir.mkdir()
            extra_args = ["--manifest-path", str(source_dir / "Cargo.toml")]
        result = subprocess.run(
            [
                sys.executable,
                str(HELPER),
                str(fake_cargo),
                str(output),
                "--locked",
                "--offline",
                "--target",
                "aarch64-unknown-linux-musl",
            ] + extra_args,
            env=env,
            capture_output=True,
            text=True,
        )
        return result, output, json.loads(command_log.read_text())

    def test_uses_reported_artifact_and_preserves_environment(self):
        with tempfile.TemporaryDirectory(prefix="pods build ") as tmp:
            directory = Path(tmp)
            result, output, log = self.run_build(directory)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output.read_text(), "executable\n")
            self.assertTrue(os.access(output, os.X_OK))
            self.assertEqual(log["home"], str(directory / "cargo cache"))
            self.assertEqual(
                log["args"],
                [
                    "build",
                    "--message-format=json-render-diagnostics",
                    "--locked",
                    "--offline",
                    "--target",
                    "aarch64-unknown-linux-musl",
                ],
            )

    def test_failed_build_does_not_copy_partial_artifact(self):
        with tempfile.TemporaryDirectory() as tmp:
            result, output, _ = self.run_build(Path(tmp), status=17)
            self.assertEqual(result.returncode, 17)
            self.assertFalse(output.exists())

    def test_reads_source_cargo_configuration_in_out_of_tree_build(self):
        with tempfile.TemporaryDirectory(prefix="pods build ") as tmp:
            directory = Path(tmp)
            result, output, log = self.run_build(directory, manifest=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(output.exists())
            self.assertEqual(log["cwd"], str(directory / "source project"))

    def test_missing_artifact_is_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            result, output, _ = self.run_build(Path(tmp), report_artifact=False)
            self.assertEqual(result.returncode, 1)
            self.assertFalse(output.exists())
            self.assertIn("without reporting", result.stderr)


if __name__ == "__main__":
    unittest.main()
