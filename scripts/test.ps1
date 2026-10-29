# Testsuite für jeden Patch: TypeScript, Rust-Unit-Tests, UI-Tests und End-to-End-Tests
# mit echten Modellen (Rede, Datei, Meeting mit vier Personen, deutscher Dialog).
#
#   .\scripts\test.ps1            alles außer Hardware
#   .\scripts\test.ps1 -Quick     ohne Modelle, in rund einer Minute
#   .\scripts\test.ps1 -Hardware  zusätzlich Mikrofon und System-Audio (spielt 11 s Ton ab)
#
# Der erste Lauf lädt Testaudio und Sortformer (~180 MB) nach src-tauri\target\test-fixtures.
# Am Ende stehen die Kennzahlen der End-to-End-Tests neben denen des letzten Laufs.
param([switch]$Quick, [switch]$Hardware)

$root = Split-Path $PSScriptRoot -Parent
$tauri = Join-Path $root 'src-tauri'
$reports = Join-Path $tauri 'target\test-reports'
. "$PSScriptRoot\env.ps1"

$results = [System.Collections.Generic.List[object]]::new()

function Step([string]$name, [string]$dir, [scriptblock]$cmd) {
    Write-Host "`n=== $name ===" -ForegroundColor Cyan
    $sw = [Diagnostics.Stopwatch]::StartNew()
    Push-Location $dir
    try {
        $global:LASTEXITCODE = 0
        & $cmd
        $ok = $LASTEXITCODE -eq 0
    } catch {
        Write-Host $_ -ForegroundColor Red
        $ok = $false
    } finally {
        Pop-Location
    }
    $results.Add([pscustomobject]@{
        Schritt  = $name
        Ergebnis = if ($ok) { 'OK' } else { 'FEHLER' }
        Dauer    = '{0:n0} s' -f $sw.Elapsed.TotalSeconds
    })
}

function Read-Metrics([string]$dir) {
    $map = @{}
    if (Test-Path $dir) {
        foreach ($file in Get-ChildItem $dir -Filter *.json) {
            $json = Get-Content $file.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($p in $json.metrics.PSObject.Properties) { $map["$($json.test)|$($p.Name)"] = [double]$p.Value }
        }
    }
    $map
}

function Format-Value($v) {
    if ($null -eq $v) { return '–' }
    if ($v -eq [math]::Round($v)) { return '{0:n0}' -f $v }
    '{0:n3}' -f $v
}

Step 'TypeScript' $root { bun x tsc --noEmit }
Step 'Rust-Unit-Tests' $tauri { cargo test --lib }
Step 'UI-Tests (Playwright)' $root { bun run test }

if (-not $Quick) {
    if (Test-Path "$reports\e2e") {
        Remove-Item -Recurse -Force "$reports\e2e-previous" -ErrorAction SilentlyContinue
        Move-Item "$reports\e2e" "$reports\e2e-previous"
    }
    if (-not (Test-Path "$tauri\target\test-fixtures\models")) {
        Write-Host 'Erster Lauf: Testaudio und Sortformer werden geladen (~180 MB).' -ForegroundColor Yellow
    }
    $e2eArgs = @('test', '--lib', 'e2e::', '--', '--ignored', '--test-threads=1')
    if (-not $Hardware) { $e2eArgs += @('--skip', 'e2e::hardware') }
    Step 'End-to-End (echte Modelle)' $tauri { cargo @e2eArgs }

    $now = Read-Metrics "$reports\e2e"
    $before = Read-Metrics "$reports\e2e-previous"
    if ($now.Count -gt 0) {
        Write-Host "`n=== Kennzahlen ===" -ForegroundColor Cyan
        $now.Keys | Sort-Object | ForEach-Object {
            $test, $metric = $_ -split '\|'
            [pscustomobject]@{
                Test      = $test
                Kennzahl  = $metric
                Vorher    = Format-Value $before[$_]
                Jetzt     = Format-Value $now[$_]
            }
        } | Format-Table -AutoSize | Out-Host
        Write-Host 'wer/der: Fehlerraten (kleiner ist besser), zuordnung: richtig zugeordnete Sprechzeit, echtzeitfaktor: Rechenzeit je Sekunde Audio.'
    }
}

Write-Host "`n=== Ergebnis ===" -ForegroundColor Cyan
$results | Format-Table -AutoSize | Out-Host
if ($results.Ergebnis -contains 'FEHLER') {
    Write-Host 'Mindestens ein Schritt ist fehlgeschlagen.' -ForegroundColor Red
    exit 1
}
Write-Host 'Alles grün.' -ForegroundColor Green
