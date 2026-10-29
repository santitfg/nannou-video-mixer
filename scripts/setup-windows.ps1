# One-time setup of the build dependencies on Windows 10/11.
# Run in PowerShell:  powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
# Safe to run again: it skips what is already installed.
#
# FFmpeg is compiled from source during the first `cargo build`. Its build
# script (vendor/ffmpeg-sys-next/build.rs) finds MSVC, MSYS2 and NASM in their
# default locations by itself, so nothing has to be added to PATH.
$ErrorActionPreference = "Stop"

# winget ships with Windows but its folder is not always on PATH.
if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    $apps = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps"
    if (-not (Test-Path (Join-Path $apps "winget.exe"))) {
        throw "winget not found: install 'App Installer' from the Microsoft Store"
    }
    $env:Path = "$env:Path;$apps"
}

function Install-Winget($id) {
    winget list --id $id -e --accept-source-agreements *> $null
    if ($LASTEXITCODE -eq 0) { Write-Host "==> $id already installed"; return }
    Write-Host "==> Installing $id"
    winget install --id $id -e --accept-source-agreements --accept-package-agreements
    if ($LASTEXITCODE -ne 0) { throw "winget install $id failed" }
}

# Rust (MSVC toolchain).
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Install-Winget Rustlang.Rustup }

# C/C++ compiler: Visual Studio or its Build Tools with the C++ workload.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vc = $null
if (Test-Path $vswhere) {
    $vc = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $vc) {
    Write-Host "==> Installing Visual Studio Build Tools (C++ workload); this takes a while"
    winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-source-agreements --accept-package-agreements `
        --override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    if ($LASTEXITCODE -ne 0) { throw "Visual Studio Build Tools install failed" }
} else {
    Write-Host "==> MSVC found in $vc"
}

Install-Winget Git.Git
Install-Winget NASM.NASM   # FFmpeg's x86 assembly
Install-Winget LLVM.LLVM   # libclang, for bindgen
Install-Winget MSYS2.MSYS2 # sh + make for FFmpeg's configure

# FFmpeg's configure needs make and cmp/diff from MSYS2.
$msys = if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { "C:\msys64" }
$bash = Join-Path $msys "usr\bin\bash.exe"
if (-not (Test-Path $bash)) { throw "MSYS2 not found in $msys (set MSYS2_ROOT if it is elsewhere)" }
Write-Host "==> Installing make and diffutils in MSYS2"
& $bash -lc "pacman -S --needed --noconfirm make diffutils"
if ($LASTEXITCODE -ne 0) { throw "pacman failed" }

# bindgen looks for libclang.dll here.
$llvm = "C:\Program Files\LLVM\bin"
if (Test-Path (Join-Path $llvm "libclang.dll")) {
    [Environment]::SetEnvironmentVariable("LIBCLANG_PATH", $llvm, "User")
} else {
    Write-Warning "libclang.dll not found in $llvm; set LIBCLANG_PATH to the folder that has it."
}

Write-Host "==> Done. Open a NEW terminal and run: cargo run"
