#!/usr/bin/env python3
"""Install the official portable Real-ESRGAN Vulkan runtime and model weights."""

import argparse
from pathlib import Path, PurePosixPath
import platform
import shutil
import stat
import tempfile
import urllib.request
import zipfile

RELEASE = "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0"
PACKAGE = "realesrgan-ncnn-vulkan-20220424-{}"
DEFAULT_DESTINATION = Path(__file__).resolve().parents[1] / "tools/runtimes/ncnn"


def runtime_platform(system=None, machine=None):
    system = system or platform.system()
    machine = (machine or platform.machine()).lower()
    if system == "Darwin" and machine in ("arm64", "aarch64", "x86_64", "amd64"):
        return "macos"
    if machine in ("x86_64", "amd64") and system in ("Linux", "Windows"):
        return {"Linux": "ubuntu", "Windows": "windows"}[system]
    raise ValueError(
        f"No official prebuilt runtime for {system}/{machine}. Build Real-ESRGAN-ncnn-vulkan "
        "from source and set SUPER_PUPER_NCNN_BINARY and SUPER_PUPER_NCNN_MODELS."
    )


def extract_runtime(archive, destination, target):
    binary_name = "realesrgan-ncnn-vulkan" + (".exe" if target == "windows" else "")
    with zipfile.ZipFile(archive) as bundle:
        # Validate the entire archive before writing any member.
        for member in bundle.infolist():
            path = PurePosixPath(member.filename)
            mode = member.external_attr >> 16
            if path.is_absolute() or ".." in path.parts or "\\" in member.filename or ":" in member.filename or stat.S_ISLNK(mode):
                raise ValueError(f"Unsafe archive entry: {member.filename}")
        binaries = [m for m in bundle.infolist() if PurePosixPath(m.filename).name == binary_name]
        if len(binaries) != 1:
            raise ValueError(f"Archive must contain exactly one {binary_name}")
        root = PurePosixPath(binaries[0].filename).parent
        for member in bundle.infolist():
            path = PurePosixPath(member.filename)
            if member.is_dir() or not path.is_relative_to(root):
                continue
            relative = path.relative_to(root)
            output = destination.joinpath(*relative.parts)
            output.parent.mkdir(parents=True, exist_ok=True)
            with bundle.open(member) as source, output.open("wb") as sink:
                shutil.copyfileobj(source, sink)
    binary = destination / binary_name
    binary.chmod(binary.stat().st_mode | 0o111)
    for name in ("realesrgan-x4plus", "realesrgan-x4plus-anime", "realesr-animevideov3-x4"):
        for extension in ("param", "bin"):
            if not (destination / "models" / f"{name}.{extension}").is_file():
                raise ValueError(f"Archive is missing the bundled model {name}.{extension}")


def install(destination, target, archive=None):
    destination = destination.resolve()
    if destination.exists():
        raise ValueError(f"Destination already exists: {destination}. Choose a new --destination.")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="ncnn-install-", dir=destination.parent) as working:
        working = Path(working)
        if archive is None:
            archive = working / "runtime.zip"
            url = f"{RELEASE}/{PACKAGE.format(target)}.zip"
            print(f"Downloading {url}", flush=True)
            request = urllib.request.Request(url, headers={"User-Agent": "super-puper-image-scaler"})
            with urllib.request.urlopen(request, timeout=120) as response, archive.open("wb") as sink:
                shutil.copyfileobj(response, sink)
        staged = working / "runtime"
        staged.mkdir()
        extract_runtime(archive, staged, target)
        staged.rename(destination)
    print(f"Installed Vulkan runtime and models: {destination}")
    print("Restart the app and select a model ending in 'Vulkan'.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, default=DEFAULT_DESTINATION)
    parser.add_argument("--archive", type=Path, help="Use an already downloaded official ZIP")
    parser.add_argument("--platform", choices=("macos", "ubuntu", "windows"), help="Prepare a bundle for another OS")
    args = parser.parse_args()
    try:
        install(args.destination, args.platform or runtime_platform(), args.archive)
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        parser.exit(1, f"Installation failed: {error}\n")


if __name__ == "__main__":
    main()
