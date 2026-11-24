#!/usr/bin/env python3
"""Archive a prepared initramfs without requiring host root or device nodes."""

import argparse
import gzip
import os
from pathlib import Path
import stat

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("root", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()

with gzip.open(args.output, "wb", compresslevel=1) as archive:
    inode = 0

    def entry(name, mode, data=b"", rdev_major=0, rdev_minor=0):
        global inode
        inode += 1
        encoded_name = name.encode() + b"\0"
        fields = [inode, mode, 0, 0, 1, 0, len(data), 0, 0,
                  rdev_major, rdev_minor, len(encoded_name), 0]
        header = b"070701" + b"".join(f"{field:08x}".encode() for field in fields)
        archive.write(header + encoded_name)
        archive.write(b"\0" * (-(len(header) + len(encoded_name)) % 4))
        archive.write(data)
        archive.write(b"\0" * (-len(data) % 4))

    for path in sorted(args.root.rglob("*")):
        mode = path.lstat().st_mode
        name = str(path.relative_to(args.root))
        if name in ("dev/console", "dev/null"):
            continue
        if stat.S_ISLNK(mode):
            data = os.readlink(path).encode()
        elif stat.S_ISREG(mode):
            data = path.read_bytes()
        elif stat.S_ISDIR(mode):
            data = b""
        else:
            continue
        entry(name, mode, data)
    entry("dev/console", stat.S_IFCHR | 0o600, rdev_major=5, rdev_minor=1)
    entry("dev/null", stat.S_IFCHR | 0o666, rdev_major=1, rdev_minor=3)
    entry("TRAILER!!!", 0)

print(f"{args.output}: {args.output.stat().st_size} bytes")
