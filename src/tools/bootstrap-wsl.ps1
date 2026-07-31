param(
    [string]$Distro = "Ubuntu",
    [string]$RepoPath = "~/src/MattOS",
    [switch]$SkipPackageInstall,
    [switch]$BuildIso
)

$ErrorActionPreference = "Stop"

Write-Host "MattOS Windows -> WSL bootstrap" -ForegroundColor Cyan

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "cargo was not found in PATH on Windows host." -ForegroundColor Red
    exit 1
}

$cmd = @("run", "-p", "mattos-build", "--", "bootstrap-wsl", "--distro", $Distro, "--repo-path", $RepoPath)
if ($SkipPackageInstall) {
    $cmd += "--skip-package-install"
}

Write-Host "Running: cargo $($cmd -join ' ')" -ForegroundColor Gray
& cargo @cmd
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

if ($BuildIso) {
    $buildCmd = @("run", "-p", "mattos-build", "--", "build-wsl-iso", "--distro", $Distro, "--repo-path", $RepoPath)
    Write-Host "Running: cargo $($buildCmd -join ' ')" -ForegroundColor Gray
    & cargo @buildCmd
    exit $LASTEXITCODE
}

Write-Host "Bootstrap complete. Run: cargo run -p mattos-build -- build-wsl-iso" -ForegroundColor Green
