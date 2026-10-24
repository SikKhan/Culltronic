"""Exercise the runtime audit's rejection paths with minimal PE fixtures."""

import importlib.util
from pathlib import Path
import struct
import json
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("packager", Path(__file__).with_name("package-windows.py"))
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


class ImportAuditTests(unittest.TestCase):
    def audit(self, library=b"kernel32.dll", *, machine=0x8664, subsystem=2, delayed=False):
        data = bytearray(2048)
        struct.pack_into("<I", data, 0x3C, 128)
        data[128:132] = b"PE\0\0"
        struct.pack_into("<HH", data, 132, machine, 1)
        struct.pack_into("<H", data, 148, 240)
        optional = 152
        struct.pack_into("<H", data, optional, 0x20B)
        struct.pack_into("<H", data, optional + 68, subsystem)
        struct.pack_into("<IIII", data, optional + 240 + 8, 1024, 0x1000, 1024, 512)
        directory = 13 if delayed else 1
        struct.pack_into("<II", data, optional + 112 + directory * 8, 0x1000, 64)
        if delayed:
            struct.pack_into("<II", data, 512, 1, 0x1100)
        else:
            struct.pack_into("<I", data, 512 + 12, 0x1100)
        data[768:768 + len(library)] = library
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "sample.exe"
            binary.write_bytes(data)
            return packager.pe_imports(binary)

    def test_accepts_system_imports(self):
        self.assertEqual(self.audit(), ["kernel32.dll"])

    def test_rejects_external_runtime_in_normal_and_delay_imports(self):
        for delayed in (False, True):
            with self.assertRaisesRegex(ValueError, "Unbundled runtime"):
                self.audit(b"vcruntime140.dll", delayed=delayed)

    def test_rejects_console_executable(self):
        with self.assertRaisesRegex(ValueError, "GUI subsystem"):
            self.audit(subsystem=3)

    def test_rejects_wrong_architecture(self):
        with self.assertRaisesRegex(ValueError, "Windows x64"):
            self.audit(machine=0x14C)

    def test_locked_dependencies_all_have_license_texts(self):
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", packager.TARGET],
            cwd=packager.ROOT))
        reachable = {node["id"] for node in metadata["resolve"]["nodes"]}
        with tempfile.TemporaryDirectory() as temporary:
            for dependency in metadata["packages"]:
                if dependency["id"] not in reachable or dependency["source"] is None:
                    continue
                destination = Path(temporary) / f'{dependency["name"]}-{dependency["version"]}'
                packager.collect_license_files(dependency, destination)
                if dependency["name"] == "epaint_default_fonts":
                    self.assertTrue((destination / "fonts/OFL.txt").is_file())
                    self.assertTrue((destination / "fonts/UFL.txt").is_file())
                    self.assertTrue((destination / "fonts/Hack-Regular.txt").is_file())


if __name__ == "__main__":
    unittest.main()
