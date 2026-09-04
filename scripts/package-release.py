#!/usr/bin/env python3
"""Package a native release binary in cargo-binstall's default archive layout."""

import hashlib
import pathlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile


target = sys.argv[1]
with open("Cargo.toml", "rb") as manifest:
    version = tomllib.load(manifest)["package"]["version"]
name = f"systemless-{target}-v{version}"
windows = "windows" in target
binary = "systemless.exe" if windows else "systemless"
dist = pathlib.Path("dist")
dist.mkdir(exist_ok=True)
archive = dist / (name + (".zip" if windows else ".tgz"))

with tempfile.TemporaryDirectory() as temporary:
    root = pathlib.Path(temporary)
    package = root / name
    package.mkdir()
    shutil.copy2(pathlib.Path("target") / target / "release" / binary, package)
    for filename in ("LICENSE", "OFL.txt", "README.md"):
        shutil.copy2(filename, package)
    if windows:
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for path in sorted(package.iterdir()):
                output.write(path, f"{name}/{path.name}")
    else:
        with tarfile.open(archive, "w:gz") as output:
            output.add(package, arcname=name)
    # Run the executable extracted from the archive to check both the package
    # layout and native runtime dependencies without requiring a GUI display.
    extracted = root / "extracted"
    if windows:
        with zipfile.ZipFile(archive) as source:
            source.extractall(extracted)
    else:
        with tarfile.open(archive) as source:
            source.extractall(extracted, filter="data")
    subprocess.run([str(extracted / name / binary), "--help"], check=True)

checksum = hashlib.file_digest(archive.open("rb"), "sha256").hexdigest()
archive.with_name(archive.name + ".sha256").write_text(
    f"{checksum}  {archive.name}\n", encoding="utf-8"
)
print(archive)
