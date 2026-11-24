#!/usr/bin/env python3
"""Build Pods and copy Cargo's reported executable to the Meson output."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cargo")
    parser.add_argument("output", type=Path)
    parser.add_argument("cargo_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    # Cargo reports the real path even when build.target is set in a config
    # file or CARGO_BUILD_TARGET. Do not guess a native release/ subdirectory.
    command = [args.cargo, "build", "--message-format=json-render-diagnostics"]
    command.extend(args.cargo_args)
    output = args.output.resolve()
    source_dir = None
    if "--manifest-path" in args.cargo_args:
        index = args.cargo_args.index("--manifest-path")
        source_dir = Path(args.cargo_args[index + 1]).resolve().parent
    executable = None
    with subprocess.Popen(
        command, cwd=source_dir, stdout=subprocess.PIPE, text=True
    ) as process:
        for line in process.stdout:
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                print(line, end="")
                continue

            if message.get("reason") == "compiler-message":
                rendered = message.get("message", {}).get("rendered")
                if rendered:
                    print(rendered, end="", file=sys.stderr)
            elif message.get("reason") == "compiler-artifact":
                target = message.get("target", {})
                if target.get("name") == "pods" and "bin" in target.get("kind", []):
                    executable = message.get("executable")

        status = process.wait()

    if status:
        return status
    if not executable:
        print("Cargo succeeded without reporting the Pods executable", file=sys.stderr)
        return 1

    shutil.copy2(executable, output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
