# Windows portable package validation

Issue: https://github.com/SikKhan/Culltronic/issues/2

Record the exact ZIP filename, SHA-256, commit, OS build, account privileges,
GPU/driver, monitor arrangement and scaling. Repeat affected checks after any
change to the package. Do not mark these passed from hosted CI alone.

## Automated evidence

CI runs format, Clippy and workspace tests on Ubuntu and Windows, builds the
Windows release ZIP, checks its x64 GUI PE header and system DLL imports, then
extracts it into a path with spaces/non-English characters and requests a normal
window launch/shutdown. Package BUILD-INFO.json records the source commit and
import audit. Dynamically loaded GPU libraries still require runtime validation.

The static CRT build needs no MSVC redistributable installer. wgpu's default
DX12 compiler selection falls back to system FXC when DXC is absent. Validate
that fallback on the clean machine, where development tools/DXC are absent.

## Clean Windows 10/11 x64, standard user

- [ ] Record package identity and verify its checksum before extraction.
- [ ] No Rust, MSVC tools or separately installed application runtime required.
- [ ] Extract and launch from C:, another drive and USB; no elevation or console.
- [ ] Also run with the application directory read-only; profile stays writable.
- [ ] Open space/non-English photo paths using both picker and drag-in.
- [ ] Browse real RAW samples; inspect previews, error tiles, labels and filters.
- [ ] Rotate, restart, reopen and confirm labels/rotation/settings/recents survive.
- [ ] Reopen unchanged folders and verify cached previews are reused.
- [ ] Drag a folder, a RAW, multiple RAWs and JPEG companions; verify focus and selection.
- [ ] Drop both pair members and verify one selected tile; unpaired JPEG gives feedback.
- [ ] Reject multiple folders, mixed folders/files, different-parent photos and unsupported files.
- [ ] Rapid successive photo drops never apply an older scan/selection.
- [ ] Drag over without dropping, then leave; feedback clears.
- [ ] Click/Ctrl-click/Shift-click/box selection and shortcuts still work.
- [ ] Drop during dialogs/export shows a message without interrupting the operation.
- [ ] Export RAW/JPEG/both; test destination picker cancellation and overwrite confirmation.
- [ ] Cancel export, remove a source and deny destination access; report accurate results.
- [ ] Browse/remove an external drive; missing/unreadable sources do not freeze the UI.
- [ ] Resize/fullscreen, close/reopen, disconnect a monitor and restore on-screen.
- [ ] Test 100/125/150/200% DPI and transitions between mixed-DPI monitors.
- [ ] Close app, move/replace extracted files, relaunch; all profile data survives.
- [ ] If prior roaming window state exists, confirm first-run migration and backup retention.
- [ ] Confirm same-commit Ubuntu CI and Linux packaging remain successful.

## Current status

Implementation and automated evidence belong in the linked PR/issue. This
checklist intentionally contains no pre-claimed manual or clean-machine passes.
The issue remains open until evidence for the exact packaged ZIP is recorded.
