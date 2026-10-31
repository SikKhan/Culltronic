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
- [ ] Repeatedly drag the window smaller in the contact sheet and loupe with
      a photo loaded; enlarge again and verify navigation remains responsive.
- [ ] Test 100/125/150/200% DPI and transitions between mixed-DPI monitors.
- [ ] Close app, move/replace extracted files, relaunch; all profile data survives.
- [ ] If prior roaming window state exists, confirm first-run migration and backup retention.
- [ ] Confirm same-commit Ubuntu CI and Linux packaging remain successful.

## Current status

On 31 October 2026, the user reported two resize freezes followed by crashes
in the contact sheet and loupe. Local Windows Application events recorded
native exceptions `0x0000087d` followed by `0xc000041d`. The machine has an
Intel Arc B580 with driver 32.0.101.8864. This matches the symptoms in
[wgpu #9911](https://github.com/gfx-rs/wgpu/issues/9911), which reports an
Intel Arc Vulkan resize crash and working DX12 rendering.

Windows startup now defaults to DX12; other platforms retain eframe defaults.
An explicit `WGPU_BACKEND` still overrides this for diagnostics. Startup logs
record the selected adapter and backend. If graphics initialization fails
before app creation, Windows retries Vulkan, then OpenGL. Successful exit,
errors after app creation, and non-graphics errors never trigger a retry.
Explicit overrides disable fallback. Regressions cover retry order, success,
exhaustion, overrides, and non-retry cases. The Windows smoke test
`cargo run -p culltronic-ui --example verify_graphics_fallback` deliberately
rejects DX12 adapter selection, then creates and renders an actual Vulkan
window using the same event loop. It passed locally on the B580.
The local debug build was confirmed to use the B580/DX12 and survived narrow
contact-sheet and loupe resizing, including a loaded photo at approximately
350 by 230 screen pixels, with next-photo navigation still working. Large to
tiny maximize/restore cycles also stayed responsive. Automated resize attempts
with the previous default and explicitly forced Vulkan did not reproduce the
intermittent crash. The user subsequently confirmed that the DX12 mitigation
works. This is not clean-machine certification or evidence for startup on
physical hardware without DX12 support; fallback control flow was tested by
injecting an adapter-selection failure.

On 26 October 2026, the user reported that clean Windows testing and complete
interaction testing worked without hiccups. Record these as user-reported passes;
the report did not include the tested ZIP checksum, OS builds or account details,
so the individual evidence boxes above remain available for that information.
This supersedes the earlier report that clean-machine testing was blocked.

The user separately reported an export failure: Ctrl-selecting two photos and
clicking Export copied the whole folder. A full egui input regression reproduced
the failure: the floating export control's press reached the underlying sheet's
marquee handler, clearing selection before Export resolved its file list.
Marquee presses now require the sheet's layer to own the pointer location.
The regression exercises Ctrl-selection, the export scope menu and Export in
RAW-only, JPEG-only and RAW+JPEG modes, and verifies that bare-sheet clicks still
clear selection. The updated packaged app requires an export recheck; the user's
earlier manual passes describe the previous candidate.

Implementation and automated evidence belong in the linked PR/issue. Keep the
issue open while export validation and the remaining release evidence are pending.
