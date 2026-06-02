$crates = @(
    "senka-core",
    "senka-secrets",
    "senka-runner",
    "senka-store",
    "senka-tui",
    "senka"
)

$waitSeconds = 10

foreach ($crate in $crates) {
    Write-Host "Publishing $crate..." -ForegroundColor Cyan
    cargo publish -p $crate

    if ($LASTEXITCODE -ne 0) {
        Write-Host "Failed to publish $crate (exit code $LASTEXITCODE). Aborting." -ForegroundColor Red
        exit $LASTEXITCODE
    }

    Write-Host "Published $crate successfully." -ForegroundColor Green

    if ($crate -ne $crates[-1]) {
        Write-Host "Waiting $waitSeconds seconds before next publish..." -ForegroundColor Yellow
        Start-Sleep -Seconds $waitSeconds
    }
}

Write-Host "All crates published successfully!" -ForegroundColor Green
