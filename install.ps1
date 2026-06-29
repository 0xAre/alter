# ALTER Installer — Windows x64
# Repo ini private. Butuh gh CLI + gh auth login sebagai collaborator.
#
# Install command:
#   $t=(gh auth token); irm -H @{Authorization="Bearer $t"} https://raw.githubusercontent.com/0xAre/alter/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$repo       = "0xAre/alter"
$installDir = "$env:LOCALAPPDATA\Programs\alter"
$binary     = "alter.exe"

function Write-Step { param($msg) Write-Host ("  >> " + $msg) -ForegroundColor Cyan }
function Write-Ok   { param($msg) Write-Host ("  OK " + $msg) -ForegroundColor Green }
function Write-Warn { param($msg) Write-Host ("  !! " + $msg) -ForegroundColor Yellow }
function Write-Err  { param($msg) Write-Host ("  ERROR: " + $msg) -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host "  ALTER Installer" -ForegroundColor White
Write-Host "  ==============================" -ForegroundColor DarkGray
Write-Host ""

# Cek gh CLI
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    Write-Err "gh CLI tidak ditemukan. Install: https://cli.github.com, lalu: gh auth login"
}

# Ambil release terbaru via gh (auth otomatis)
Write-Step "Mengecek release terbaru..."
try {
    $release = gh api repos/$repo/releases/latest | ConvertFrom-Json
} catch {
    Write-Err "Gagal akses release. Pastikan sudah 'gh auth login' dan kamu adalah collaborator."
}

$version = $release.tag_name
Write-Ok "Versi: $version"

# Cari asset Windows .exe
$asset = $release.assets | Where-Object { $_.name -like "*windows*x86_64*.exe" }
if (-not $asset) {
    $asset = $release.assets | Where-Object { $_.name -like "*windows*msvc*.exe" } | Select-Object -First 1
}
if (-not $asset) {
    $asset = $release.assets | Where-Object { $_.name -like "*.exe" } | Select-Object -First 1
}
if (-not $asset) {
    Write-Err "Tidak ada binary .exe di release $version. Assets: $($release.assets.name -join ', ')"
}

$sizeMB = [math]::Round($asset.size / 1MB, 1)
Write-Step ("Download " + $asset.name + " (" + $sizeMB + " MB)...")

# Buat folder install
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
$destPath = Join-Path $installDir $binary

# Download via gh (handles auth untuk private repo)
$tmp = Join-Path $env:TEMP $asset.name
gh release download $version --repo $repo --pattern $asset.name --dir $env:TEMP --clobber 2>&1 | Out-Null

if (-not (Test-Path $tmp)) {
    Write-Err "Download gagal."
}

Move-Item $tmp $destPath -Force
Write-Ok "Installed: $destPath"

# Tambah ke PATH user (tidak butuh admin)
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if (-not $userPath) { $userPath = "" }
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$installDir", "User")
    Write-Ok "PATH diperbarui."
} else {
    Write-Warn "PATH sudah ada, tidak diubah."
}

Write-Host ""
Write-Host "  Selesai! Buka terminal baru lalu ketik: alter" -ForegroundColor Green
Write-Host ""
