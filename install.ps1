<#
.SYNOPSIS
    Install ALTER — P2P encrypted terminal chat (Windows x64)
.DESCRIPTION
    Download binary terbaru dari GitHub Releases dan tambahkan ke PATH.
    Tidak membutuhkan Rust atau dependency lain.
.EXAMPLE
    irm https://raw.githubusercontent.com/0xAre/alter/main/install.ps1 | iex
.EXAMPLE
    # Install versi spesifik
    $env:ALTER_VERSION = "v0.1.3"
    irm https://raw.githubusercontent.com/0xAre/alter/main/install.ps1 | iex
#>

$ErrorActionPreference = "Stop"

$repo      = "0xAre/alter"
$assetName = "alter-x86_64-pc-windows-msvc.exe"
$installDir = "$env:LOCALAPPDATA\Programs\alter"
$binary    = "alter.exe"

function Write-Step($msg) { Write-Host "  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "  $msg" -ForegroundColor Green }
function Write-Err($msg)  { Write-Host "  ERROR: $msg" -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host " ALTER Installer" -ForegroundColor White
Write-Host " ===============" -ForegroundColor DarkGray
Write-Host ""

# Tentukan versi yang akan diinstall
if ($env:ALTER_VERSION) {
    $version = $env:ALTER_VERSION.TrimStart("v")
    Write-Step "Versi ditentukan: v$version"
    $apiUrl = "https://api.github.com/repos/$repo/releases/tags/v$version"
} else {
    Write-Step "Mengecek versi terbaru..."
    $apiUrl = "https://api.github.com/repos/$repo/releases/latest"
}

try {
    $release = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing
} catch {
    Write-Err "Gagal mengambil informasi release dari GitHub.`nPastikan repo sudah publik atau coba lagi nanti.`nDetail: $_"
}

$version = $release.tag_name
Write-Ok "Versi: $version"

# Cari asset yang sesuai
$asset = $release.assets | Where-Object { $_.name -eq $assetName }
if (-not $asset) {
    Write-Err "Binary '$assetName' tidak ditemukan di release $version.`nAsset yang tersedia: $($release.assets.name -join ', ')"
}

# Buat direktori instalasi
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

$destPath = Join-Path $installDir $binary

Write-Step "Mengunduh $assetName ($([math]::Round($asset.size / 1MB, 1)) MB)..."
try {
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $destPath -UseBasicParsing
} catch {
    Write-Err "Gagal mengunduh binary: $_"
}
Write-Ok "Binary tersimpan: $destPath"

# Tambah ke PATH pengguna (persistent, tidak butuh admin).
# Hindari operator '??' — tidak ada di Windows PowerShell 5.1 (default laptop kosong).
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if (-not $userPath) { $userPath = "" }
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$installDir", "User")
    Write-Ok "Ditambahkan ke PATH: $installDir"
} else {
    Write-Step "PATH sudah berisi $installDir, tidak perlu diubah."
}

Write-Host ""
Write-Host " Instalasi selesai!" -ForegroundColor Green
Write-Host ""
Write-Host " Tutup dan buka ulang terminal, lalu jalankan:" -ForegroundColor DarkGray
Write-Host "   alter" -ForegroundColor White
Write-Host ""
