# One-time setup of the build dependencies on Windows 11 (run in PowerShell).
# Also needs the "Desktop development with C++" workload of Visual Studio Build Tools.
$ErrorActionPreference = "Stop"

winget install --id Git.Git -e
winget install --id NASM.NASM -e
winget install --id LLVM.LLVM -e
winget install --id MSYS2.MSYS2 -e

# FFmpeg's configure is a shell script: it needs MSYS2's make on PATH.
C:\msys64\usr\bin\bash.exe -lc "pacman -S --noconfirm make diffutils"

[Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
$add = "C:\Program Files\NASM;C:\msys64\usr\bin"
$path = [Environment]::GetEnvironmentVariable("Path", "User")
[Environment]::SetEnvironmentVariable("Path", "$path;$add", "User")

Write-Host "==> Done. Open a NEW terminal and run: cargo run"
