Culltronic for Windows x64 (portable)

Extract the complete ZIP, then double-click culltronic-ui.exe.
No installer or administrator access is needed. No Rust or development tools
are needed to run the application. The target is Windows 10/11 x64 with working
DirectX 12 graphics drivers; clean-machine validation is tracked in issue #2.

Your index, labels, rotations, settings, recent folders, cached previews and
thumbnails are in %LOCALAPPDATA%\cullr. Window state is window.ron there.
The cullr name is intentional: it preserves data from earlier versions.
Existing roaming Culltronic window state is copied once; its old file is kept.
The application folder contains program files, not your culling data.

You can move the extracted application to another drive or USB stick.
To upgrade, close Culltronic and replace its extracted program files.
Your profile data remains in place. Do not delete %LOCALAPPDATA%\cullr if you
want to keep labels, rotations, settings and recent folders.

Drop one folder to browse it. Drop a RAW photo to open its folder and select it.
Drop multiple photos from one folder to select them. Companion JPEGs select
their RAW+JPEG tile. Unsupported or ambiguous drops show an explanation.
Use Export (Ctrl+E) to copy selected originals, or the filtered view if nothing
is selected. Its scope menu chooses RAW, JPEG or both. Native drag-out is not
provided. Click, Ctrl-click, Shift-click and box selection remain available.

Verify the download: in PowerShell run Get-FileHash <download.zip> -Algorithm
SHA256 and compare it with the adjacent .zip.sha256 download.

License: GPL-3.0-or-later. See LICENSE, SOURCE.txt and licenses/.
THIRD-PARTY-NOTICES.json records dependency versions and declared licenses.
BUILD-INFO.json identifies this build and its audited Windows system imports.
Report problems: https://github.com/SikKhan/Culltronic/issues
