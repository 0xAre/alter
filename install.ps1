# ALTER Installer — Windows x64
#
# Install command:
#   irm https://raw.githubusercontent.com/0xAre/alter/main/install.ps1 | iex

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

# Ambil release terbaru via GitHub API (public)
Write-Step "Mengecek release terbaru..."
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing
} catch {
    Write-Err "Gagal akses GitHub API. Cek koneksi internet: $_"
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

# Download langsung dari GitHub Releases
$tmp = Join-Path $env:TEMP $asset.name
try {
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tmp -UseBasicParsing
} catch {
    Write-Err "Download gagal: $_"
}

if (-not (Test-Path $tmp)) {
    Write-Err "Download gagal — file tidak ditemukan."
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
