# Hosted smoke test only; clean-machine and manual interaction evidence is separate.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$archive = Get-ChildItem -LiteralPath (Join-Path $root 'dist') -Filter '*-windows-x86_64.zip' |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $archive) { throw 'No Windows archive found' }
$expectedHash = ((Get-Content -LiteralPath ($archive.FullName + '.sha256')).Trim() -split '\s+')[0]
if ((Get-FileHash -LiteralPath $archive.FullName -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'Archive checksum mismatch'
}
$scratch = Join-Path ([System.IO.Path]::GetTempPath()) ('Culltronic 测试 ' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
$process = $null
try {
    Expand-Archive -LiteralPath $archive.FullName -DestinationPath $scratch
    $exe = Get-ChildItem -LiteralPath $scratch -Recurse -Filter culltronic-ui.exe | Select-Object -First 1
    if (-not $exe) { throw 'Executable missing from ZIP' }
    $process = Start-Process -FilePath $exe.FullName -WorkingDirectory $scratch -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
        if ($process.HasExited) { throw "Packaged app exited before showing its window: $($process.ExitCode)" }
    } until ($process.MainWindowHandle -ne 0 -or [DateTime]::UtcNow -ge $deadline)
    if ($process.MainWindowHandle -eq 0) { throw 'Packaged app did not create a window' }
    if (-not $process.CloseMainWindow()) { throw 'Could not request normal app shutdown' }
    if (-not $process.WaitForExit(15000)) { throw 'Packaged app did not close normally' }
    if ($process.ExitCode -ne 0) { throw "Packaged app failed: $($process.ExitCode)" }
    Write-Output "Extracted ZIP launched and closed successfully: $($archive.Name)"
} finally {
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    # Verify the generated target before recursive cleanup, entirely in PowerShell.
    $resolved = [System.IO.Path]::GetFullPath($scratch)
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if (-not $resolved.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not (Split-Path -Leaf $resolved).StartsWith('Culltronic 测试 ')) {
        throw 'Refusing cleanup outside the generated smoke-test folder'
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
