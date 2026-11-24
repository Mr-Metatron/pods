#!/usr/bin/env python3
"""Export a committed source snapshot and a checksum-pinned native APKBUILD."""

import argparse
import gzip
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib


REPOSITORY = Path(__file__).resolve().parents[1]


def git(*args):
    return subprocess.check_output(["git", "-C", str(REPOSITORY), *args])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", default="HEAD", help="Committed source ref (default: HEAD)")
    parser.add_argument("--output", required=True, type=Path, help="Destination aport directory")
    args = parser.parse_args()

    commit = git("rev-parse", "--verify", f"{args.ref}^{{commit}}").decode().strip()
    manifest = tomllib.loads(git("show", f"{commit}:Cargo.toml").decode())
    date = git("show", "-s", "--format=%cs", commit).decode().strip().replace("-", "")
    version = f"{manifest['package']['version']}_git{date}"
    source = f"pods-{version}-{commit[:12]}.tar.gz"
    template = git("show", f"{commit}:packaging/alpine/APKBUILD.in").decode()

    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="pods-alpine-") as temporary:
        archive = Path(temporary) / "source.tar"
        subprocess.run(
            [
                "git", "-C", str(REPOSITORY), "archive", "--format=tar",
                f"--prefix=pods-{version}/", "--output", str(archive), commit,
            ],
            check=True,
        )
        destination = args.output / source
        # git archive fixes entry times to the commit; gzip must not add the
        # local output path or wall-clock time to an otherwise repeatable file.
        with archive.open("rb") as raw, destination.open("wb") as compressed:
            with gzip.GzipFile(filename="", fileobj=compressed, mode="wb", mtime=0) as stream:
                shutil.copyfileobj(raw, stream)

    with destination.open("rb") as compressed:
        digest = hashlib.file_digest(compressed, "sha512").hexdigest()
    for key, value in {
        "VERSION": version,
        "COMMIT": commit,
        "SOURCE": source,
        "SHA512": digest,
    }.items():
        template = template.replace(f"@{key}@", value)
    (args.output / "APKBUILD").write_text(template)
    print(f"Source commit: {commit}")
    print(f"Source archive: {destination.resolve()}")
    print(f"APKBUILD: {(args.output / 'APKBUILD').resolve()}")
    print(f"SHA-512: {digest}")


if __name__ == "__main__":
    main()
