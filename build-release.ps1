[CmdletBinding()]
param()

# Release build of FitLedger.exe - the same script runs locally and on GitHub:
# tests, build without local paths inside the binary, checks, SHA-256.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$crateRoot = (Resolve-Path $PSScriptRoot).Path.TrimEnd('\')
$userRoot = if ($env:USERPROFILE) {
    $env:USERPROFILE.TrimEnd('\')
} elseif ($env:HOMEDRIVE -and $env:HOMEPATH) {
    ($env:HOMEDRIVE + $env:HOMEPATH).TrimEnd('\')
} else {
    [Environment]::GetFolderPath("UserProfile").TrimEnd('\')
}
$cargoRoot = if ($env:CARGO_HOME) {
    (Resolve-Path $env:CARGO_HOME).Path.TrimEnd('\')
} else {
    (Join-Path $userRoot ".cargo").TrimEnd('\')
}
$targetRoot = (Join-Path $crateRoot "target").TrimEnd('\')

# GNU toolchain: gcc (bundled SQLite) and windres (icon/manifest) come from
# MinGW-w64. Use the MSYS2 copy when they are not on the PATH yet.
if (-not (Get-Command windres -ErrorAction SilentlyContinue)) {
    foreach ($dir in @("C:\msys64\ucrt64\bin", "C:\msys64\mingw64\bin", "C:\mingw64\bin")) {
        if (Test-Path (Join-Path $dir "windres.exe")) {
            $env:PATH = "$dir;$env:PATH"
            break
        }
    }
}
if (-not (Get-Command windres -ErrorAction SilentlyContinue)) {
    throw "windres (MinGW-w64) not found. Install MSYS2 with mingw-w64 gcc/binutils."
}

# Cargo uses ASCII unit separator so paths containing spaces remain one rustc argument.
$separator = [char]0x1f
$remapFlags = @(
    "--remap-path-prefix=$userRoot=home"
    "--remap-path-prefix=$cargoRoot=cargo"
    "--remap-path-prefix=$crateRoot=src"
    "--remap-path-prefix=$targetRoot=target"
)
$env:CARGO_ENCODED_RUSTFLAGS = $remapFlags -join $separator

Push-Location $crateRoot
try {
    cargo test --locked
    if ($LASTEXITCODE -ne 0) {
        throw "The tests failed. The release was not built."
    }

    cargo clean --release
    if ($LASTEXITCODE -ne 0) {
        throw "Could not clean old release artifacts."
    }

    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "The release build failed."
    }

    $binary = Join-Path $targetRoot "release\fitledger.exe"
    $bytes = [IO.File]::ReadAllBytes($binary)
    $singleByteText = [Text.Encoding]::GetEncoding(28591).GetString($bytes)
    $utf16Text = [Text.Encoding]::Unicode.GetString($bytes)
    $forbidden = @($userRoot, $crateRoot, $env:USERNAME, $env:COMPUTERNAME) |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and $_.Length -ge 4 } |
        Select-Object -Unique

    foreach ($value in $forbidden) {
        $comparison = [StringComparison]::OrdinalIgnoreCase
        if ($singleByteText.IndexOf($value, $comparison) -ge 0 -or
            $utf16Text.IndexOf($value, $comparison) -ge 0) {
            throw "The executable contains local build information: $value"
        }
    }

    # the manifest must ask for the user's normal rights (never administrator)
    if ($singleByteText.IndexOf('level="asInvoker"') -lt 0) {
        throw "The asInvoker manifest is missing from the executable."
    }

    $hash = (Get-FileHash -Algorithm SHA256 $binary).Hash
    $size = [math]::Round((Get-Item $binary).Length / 1MB, 1)
    Write-Host "Release verified: $binary ($size MB)"
    Write-Host "SHA256: $hash"
} finally {
    Pop-Location
}
