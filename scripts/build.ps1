# scripts/build.ps1 — Build the release binary and copy it to a
# version-stamped filename next to the original.
#
# Usage (from anywhere):
#   pwsh -File scripts/build.ps1
#   .\scripts\build.ps1                    # from the project root
#
# Result:
#   target/release/cnterminal.exe          (cargo's normal artifact)
#   target/release/CNTerminal_v0.1.0.exe   (versioned copy for distribution)
#
# Update the version once, in Cargo.toml's [package].version — the file
# name follows automatically on the next build.

$ErrorActionPreference = 'Stop'

$projectRoot = Split-Path -Parent $PSScriptRoot
$cargoToml = Get-Content "$projectRoot\Cargo.toml" -Raw

if ($cargoToml -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    Write-Error "Could not find [package].version in Cargo.toml"
    exit 1
}
$version = $matches[1]

Write-Host "Building CNTerminal v$version (release)..." -ForegroundColor Cyan

cargo build --release --manifest-path "$projectRoot\Cargo.toml"
if ($LASTEXITCODE -ne 0) {
    Write-Error "cargo build --release failed (exit $LASTEXITCODE)"
    exit $LASTEXITCODE
}

$src = "$projectRoot\target\release\cnterminal.exe"
$dst = "$projectRoot\target\release\CNTerminal_v$version.exe"

if (-not (Test-Path $src)) {
    Write-Error "Expected artifact not found: $src"
    exit 1
}

Copy-Item -Path $src -Destination $dst -Force
$sizeMB = [math]::Round((Get-Item $dst).Length / 1MB, 2)
Write-Host ""
Write-Host "OK   $dst   ($sizeMB MB)" -ForegroundColor Green
