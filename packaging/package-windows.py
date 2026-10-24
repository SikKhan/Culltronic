"""Package a Windows release, audit PE imports, and collect dependency notices."""

import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-pc-windows-msvc"
# Windows 10/11 system libraries. Everything else must be explicitly bundled/audited.
SYSTEM_DLLS = set("""advapi32 bcrypt bcryptprimitives cfgmgr32 comctl32 comdlg32
combase crypt32 d2d1 d3d11 d3d12 d3dcompiler_47 dbghelp dwmapi dwrite dxgi gdi32 hid
imm32 iphlpapi kernel32 msimg32 msvcrt ncrypt netapi32 ntdll ole32 oleaut32
opengl32 powrprof propsys psapi rpcrt4 secur32 setupapi shell32 shlwapi
ucrtbase uiautomationcore user32 userenv usp10 uxtheme version winhttp winmm winspool wintrust
ws2_32 wtsapi32 normaliz runtimeobject shcore windowscodecs""".split())


def collect_license_files(dependency, destination):
    """Collect nested notices/fonts plus reviewed supplements missing from crates."""
    folder = Path(dependency["manifest_path"]).parent
    destination.mkdir(parents=True, exist_ok=True)
    sources = {path for path in folder.rglob("*") if path.is_file() and (
        path.name.lower().startswith(("license", "licence", "copying", "notice", "copyright", "authors"))
        or path.name in ("OFL.txt", "UFL.txt")
        or ("fonts" in path.parts and path.suffix == ".txt"))}
    if dependency.get("license_file"):
        sources.add(folder / dependency["license_file"])
    for source in sources:
        relative = source.relative_to(folder)
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    supplements = ROOT / "packaging/licenses"
    manifest = json.loads((supplements / "packages.json").read_text(encoding="utf-8"))
    key = f'{dependency["name"]}-{dependency["version"]}'
    for name in manifest.get(key, []):
        shutil.copy2(supplements / name, destination / name)
    if not sources and key not in manifest:
        raise ValueError(f"Missing license texts for {key}; review its pinned upstream source")


def pe_imports(binary):
    """Read PE32+ headers/import table without depending on developer tools."""
    data = binary.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("Not a PE executable")
    machine, sections = struct.unpack_from("<HH", data, pe + 4)
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if machine != 0x8664 or struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("Expected Windows x64 PE32+")
    if struct.unpack_from("<H", data, optional + 68)[0] != 2:
        raise ValueError("Expected GUI subsystem (no extra console window)")
    section_table = optional + optional_size

    def offset(rva):
        for i in range(sections):
            entry = section_table + i * 40
            size, address, raw_size, raw = struct.unpack_from("<IIII", data, entry + 8)
            if address <= rva < address + max(size, raw_size):
                return raw + rva - address
        raise ValueError(f"Unmapped RVA {rva:x}")

    def name(rva):
        start = offset(rva)
        return data[start:data.index(b"\0", start)].decode("ascii").lower()

    imports = set()
    rva = struct.unpack_from("<I", data, optional + 112 + 8)[0]
    if rva:
        cursor = offset(rva)
        while any(data[cursor:cursor + 20]):
            imports.add(name(struct.unpack_from("<I", data, cursor + 12)[0]))
            cursor += 20
    # Delay imports require the same scrutiny as the ordinary import table.
    delayed = struct.unpack_from("<I", data, optional + 112 + 13 * 8)[0]
    if delayed:
        cursor = offset(delayed)
        while any(data[cursor:cursor + 32]):
            attributes, name_rva = struct.unpack_from("<II", data, cursor)
            if attributes != 1:
                raise ValueError("Unexpected delay-import address format")
            imports.add(name(name_rva))
            cursor += 32
    unknown = [dll for dll in imports if dll.removesuffix(".dll") not in SYSTEM_DLLS
               and not dll.startswith(("api-ms-win-", "ext-ms-win-"))]
    if unknown:
        raise ValueError(f"Unbundled runtime dependencies: {unknown}")
    return sorted(imports)


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET], cwd=ROOT))
    package = next(p for p in metadata["packages"] if p["name"] == "culltronic-ui")
    version = package["version"]
    binary = ROOT / "target" / TARGET / "release" / "culltronic-ui.exe"
    imports = pe_imports(binary)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    stem = f"culltronic-{version}-windows-x86_64"
    archive = dist / f"{stem}.zip"
    with tempfile.TemporaryDirectory(prefix="culltronic-package-") as temporary:
        staging = Path(temporary) / stem
        staging.mkdir()
        shutil.copy2(binary, staging / binary.name)
        shutil.copy2(ROOT / "LICENSE", staging / "LICENSE")
        shutil.copy2(ROOT / "packaging" / "WINDOWS-README.txt", staging / "README.txt")
        notices = staging / "licenses"
        notices.mkdir()
        shutil.copy2(ROOT / "crates/culltronic-ui/assets/fonts/OFL.txt", notices / "Inter-OFL.txt")
        shutil.copy2(ROOT / "packaging/licenses/sources.json", notices / "supplement-sources.json")
        packages = []
        reachable = {node["id"] for node in metadata["resolve"]["nodes"]}
        for dependency in metadata["packages"]:
            if dependency["id"] not in reachable or dependency["source"] is None:
                continue
            destination = notices / f'{dependency["name"]}-{dependency["version"]}'
            collect_license_files(dependency, destination)
            packages.append({key: dependency.get(key) for key in ("name", "version", "license", "repository", "source")})
        (staging / "THIRD-PARTY-NOTICES.json").write_text(json.dumps(packages, indent=2), encoding="utf-8")
        (staging / "BUILD-INFO.json").write_text(json.dumps({"version": version, "commit": commit,
            "target": TARGET, "static_crt": True, "system_imports": imports,
            "dynamic_graphics": "wgpu Auto: system DX12/FXC fallback; GPU drivers supplied by Windows/vendor"}, indent=2), encoding="utf-8")
        (staging / "SOURCE.txt").write_text(
            f"Culltronic corresponding source: https://github.com/SikKhan/Culltronic/tree/{commit}\n"
            f"Source archive: https://github.com/SikKhan/Culltronic/archive/{commit}.zip\n"
            "Cargo.lock pins dependencies; cargo vendor --locked retrieves their source.\n"
            "To rebuild or modify linked libraries (including LGPL rawler), use the source,\n"
            "Rust stable and MSVC build tools, then run packaging/build-windows.ps1.\n"
            "The resulting executable can replace this one. No signing key is required.\n",
            encoding="utf-8")
        # Registry archives can timestamp notices at Unix epoch, before ZIP's 1980 floor.
        # Write beside the destination and replace only a complete archive.
        with tempfile.NamedTemporaryFile(dir=dist, suffix=".zip", delete=False) as pending:
            pending_archive = Path(pending.name)
        try:
            with zipfile.ZipFile(pending_archive, "w", zipfile.ZIP_DEFLATED, strict_timestamps=False) as bundle:
                for path in sorted(staging.rglob("*")):
                    if path.is_file():
                        bundle.write(path, path.relative_to(Path(temporary)))
            pending_archive.replace(archive)
        finally:
            pending_archive.unlink(missing_ok=True)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix(".zip.sha256").write_text(f"{digest}  {archive.name}\n", encoding="ascii")
    print(f"Packaged {archive}\nSHA-256 {digest}\nSystem imports: {', '.join(imports)}")


if __name__ == "__main__":
    main()
