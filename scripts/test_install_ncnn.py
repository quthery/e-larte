import tempfile
import unittest
from pathlib import Path
import zipfile

from install_ncnn import install, runtime_platform


class InstallerTests(unittest.TestCase):
    def archive(self, directory, *, prefix="", extra=None, complete=True, target="macos"):
        archive = directory / "runtime.zip"
        with zipfile.ZipFile(archive, "w") as bundle:
            binary = "realesrgan-ncnn-vulkan" + (".exe" if target == "windows" else "")
            bundle.writestr(prefix + binary, b"runtime")
            if complete:
                for name in ("realesrgan-x4plus", "realesrgan-x4plus-anime", "realesr-animevideov3-x4"):
                    for extension in ("param", "bin"):
                        bundle.writestr(f"{prefix}models/{name}.{extension}", b"model")
            if extra:
                bundle.writestr(extra, b"invalid")
        return archive

    def test_installs_flat_and_nested_archives_for_all_platforms(self):
        for target in ("macos", "ubuntu", "windows"):
            for prefix in ("", "release/"):
                with self.subTest(target=target, prefix=prefix), tempfile.TemporaryDirectory() as temp:
                    root = Path(temp)
                    destination = root / "installed"
                    install(destination, target, self.archive(root, prefix=prefix, target=target))
                    self.assertTrue((destination / "models/realesrgan-x4plus.bin").is_file())
                    binary = destination / ("realesrgan-ncnn-vulkan" + (".exe" if target == "windows" else ""))
                    self.assertTrue(binary.stat().st_mode & 0o111)
                    with self.assertRaisesRegex(ValueError, "already exists"):
                        install(destination, target)

    def test_failed_installation_does_not_publish_partial_runtime(self):
        for extra, complete in (("../escaped", True), ("C:/escaped", True), ("release/../../escaped", True), (None, False)):
            with self.subTest(extra=extra), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                destination = root / "installed"
                with self.assertRaises(ValueError):
                    install(destination, "macos", self.archive(root, extra=extra, complete=complete))
                self.assertFalse(destination.exists())
                self.assertFalse((root / "escaped").exists())

    def test_architecture_selection_rejects_unsupported_prebuilt_targets(self):
        self.assertEqual(runtime_platform("Darwin", "arm64"), "macos")
        self.assertEqual(runtime_platform("Linux", "x86_64"), "ubuntu")
        self.assertEqual(runtime_platform("Windows", "AMD64"), "windows")
        with self.assertRaisesRegex(ValueError, "from source"):
            runtime_platform("Linux", "aarch64")


if __name__ == "__main__":
    unittest.main()
