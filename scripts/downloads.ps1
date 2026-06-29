# ALTER - Download Counter
# Jalankan: .\scripts\downloads.ps1

$repo = "0xAre/alter"

$releases = gh api repos/$repo/releases | ConvertFrom-Json

$total = 0
foreach ($rel in $releases) {
    $rel_total = ($rel.assets | Measure-Object -Property download_count -Sum).Sum
    $total += $rel_total

    Write-Host ""
    Write-Host ("  {0}  |  {1}" -f $rel.tag_name, $rel.name) -ForegroundColor Cyan
    Write-Host ("  Published: {0}" -f $rel.published_at) -ForegroundColor DarkGray

    foreach ($asset in $rel.assets) {
        $bar_len = [Math]::Min([int]($asset.download_count / 2), 40)
        $bar = "#" * $bar_len
        $size_kb = [Math]::Round($asset.size / 1024)
        Write-Host ("  {0,-44}  {1,5} downloads  [{2} KB]" -f $asset.name, $asset.download_count, $size_kb) -ForegroundColor White
        if ($bar_len -gt 0) {
            Write-Host ("  [{0}]" -f $bar) -ForegroundColor Green
        }
    }

    if ($rel.assets.Count -eq 0) {
        Write-Host "  (no assets)" -ForegroundColor DarkGray
    }
}

Write-Host ""
Write-Host "  ---------------------------------" -ForegroundColor DarkGray
Write-Host ("  Total semua release: {0} downloads" -f $total) -ForegroundColor Yellow
Write-Host ""
