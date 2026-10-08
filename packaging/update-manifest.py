#!/usr/bin/env python3
"""Create signed-update input metadata from one release artifact directory."""
import hashlib
import json
import os
import re

VERSION = os.environ.get("PHOTOCRAFT_VERSION", "0.0.0").lstrip("v")
CHANNEL = "preview" if "-" in VERSION else "stable"
out = []

patterns = [
    (r"photocraft-.+-windows-x64\.msi$", "windows", "x86_64", "msi"),
    (r"photocraft-.+-windows-x64-portable\.zip$", "windows", "x86_64", "portable"),
    (r"photocraft-.+-windows-x86\.msi$", "windows", "x86", "msi"),
    (r"photocraft-.+-windows-x86-portable\.zip$", "windows", "x86", "portable"),
    (r"photocraft-.+-windows-arm64\.msi$", "windows", "aarch64", "msi"),
    (r"photocraft-.+-windows-arm64-portable\.zip$", "windows", "aarch64", "portable"),
    (r"photocraft-.+-macos-universal\.dmg$", "macos", "universal", "dmg"),
    (r"photocraft-.+-linux-x86_64\.AppImage$", "linux", "x86_64", "appimage"),
    (r"photocraft-.+-linux-aarch64\.AppImage$", "linux", "aarch64", "appimage"),
    (r"photocraft-.+-freebsd-x86_64\.tar\.gz$", "freebsd", "x86_64", "tar"),
]

for name in sorted(os.listdir(".")):
    path = os.path.join(".", name)
    if not os.path.isfile(path):
        continue
    match = next(((platform, arch, kind) for pattern, platform, arch, kind in patterns if re.fullmatch(pattern, name)), None)
    if match is None:
        continue
    with open(path, "rb") as stream:
        digest = hashlib.sha256(stream.read()).hexdigest()
    platform, arch, kind = match
    out.append({"platform": platform, "arch": arch, "kind": kind, "file": name, "sha256": digest, "size": os.path.getsize(path)})

if not out:
    raise SystemExit("no recognized release assets found")

manifest = {"schema": 1, "version": VERSION, "channel": CHANNEL, "published_at": None, "minimum_supported_version": None, "assets": out}
with open("update-manifest.json", "w", encoding="utf-8", newline="\n") as stream:
    json.dump(manifest, stream, indent=2)
    stream.write("\n")
