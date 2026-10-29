# Windows only: builds the release .exe and packs it into an installer.
# Run in PowerShell from anywhere:
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows-installer.ps1
#   ... -SkipBuild    reuse target\release\nannou_sketch.exe as it is
#
# Needs the tools from scripts\setup-windows.ps1, plus Inno Setup 6 (installed
# with winget on first use).
# Output: dist\windows\ (exe + shaders + icon) and
#         dist\Video-Mixer-<version>-x64-setup.exe
param([switch]$SkipBuild)
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Bin = "nannou_sketch.exe"
$Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value

# 1. The release build (static C runtime, see .cargo\config.toml).
if (-not $SkipBuild) {
    if (-not $env:LIBCLANG_PATH -and (Test-Path "C:\Program Files\LLVM\bin\libclang.dll")) {
        $env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
    }
    Write-Host "==> cargo build --release (the first time FFmpeg takes ~10 min)"
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
$Exe = Join-Path $Root "target\release\$Bin"
if (-not (Test-Path $Exe)) { throw "$Exe not found; run without -SkipBuild" }

# 2. Stage dist\windows: the program reads .\shaders at run time.
$Stage = Join-Path $Root "dist\windows"
New-Item -ItemType Directory -Force $Stage | Out-Null
Copy-Item $Exe $Stage -Force
if (Test-Path "$Stage\shaders") { Remove-Item -Recurse -Force "$Stage\shaders" }
Copy-Item -Recurse shaders "$Stage\shaders"

# 3. icon.ico from assets\icon.png (PNG-compressed entries, Windows Vista+).
function New-Ico($png, $ico) {
    Add-Type -AssemblyName System.Drawing
    $src = [System.Drawing.Image]::FromFile($png)
    $sizes = 16, 24, 32, 48, 64, 128, 256
    $images = foreach ($s in $sizes) {
        $bmp = New-Object System.Drawing.Bitmap $s, $s
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.DrawImage($src, 0, 0, $s, $s)
        $g.Dispose()
        $ms = New-Object System.IO.MemoryStream
        $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        , $ms.ToArray()
    }
    $src.Dispose()
    $out = New-Object System.IO.MemoryStream
    $w = New-Object System.IO.BinaryWriter $out
    $w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    for ($i = 0; $i -lt $sizes.Count; $i++) {
        $s = $sizes[$i] % 256  # 0 means 256
        $w.Write([byte]$s); $w.Write([byte]$s); $w.Write([byte]0); $w.Write([byte]0)
        $w.Write([uint16]1); $w.Write([uint16]32)
        $w.Write([uint32]$images[$i].Length); $w.Write([uint32]$offset)
        $offset += $images[$i].Length
    }
    foreach ($img in $images) { $w.Write($img) }
    $w.Flush()
    [System.IO.File]::WriteAllBytes($ico, $out.ToArray())
}
New-Ico (Join-Path $Root "assets\icon.png") (Join-Path $Stage "icon.ico")

# 4. Inno Setup's compiler.
function Find-Iscc {
    $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($dir in "${env:ProgramFiles(x86)}\Inno Setup 6", "$env:ProgramFiles\Inno Setup 6", "$env:LOCALAPPDATA\Programs\Inno Setup 6") {
        if (Test-Path "$dir\ISCC.exe") { return "$dir\ISCC.exe" }
    }
    return $null
}
$Iscc = Find-Iscc
if (-not $Iscc) {
    Write-Host "==> Installing Inno Setup"
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    $winget = if ($winget) { $winget.Source } else { Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps\winget.exe" }
    & $winget install --id JRSoftware.InnoSetup -e --accept-source-agreements --accept-package-agreements
    $Iscc = Find-Iscc
    if (-not $Iscc) { throw "Inno Setup not found; install it from https://jrsoftware.org/isdl.php" }
}

# 5. The installer.
Write-Host "==> Building the installer (version $Version)"
& $Iscc /Q "/DAppVersion=$Version" "/DSourceDir=$Stage" "/DOutputDir=$(Join-Path $Root 'dist')" (Join-Path $PSScriptRoot "windows-installer.iss")
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

Write-Host "==> Done: dist\windows\$Bin  (with shaders\)"
Write-Host "==> Done: dist\Video-Mixer-$Version-x64-setup.exe"
