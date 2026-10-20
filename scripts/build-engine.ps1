#Requires -Version 5.1
<#
Build du moteur Tatami : installDist + runtime jlink réduit + lanceur.
Sortie : engine/dist/tatami-engine/ (runtime/, lib/, tatami-engine.bat).
Ni le runtime ni les JAR ne sont commités (voir .gitignore : dist/, build/).
Note : les exécutables Java/Gradle écrivent sur stderr -> ErrorActionPreference
reste à Continue ici, les échecs sont détectés via $LASTEXITCODE + throw explicites.
.EXAMPLE
  .\scripts\build-engine.ps1
#>
param(
    [string]$JavaHome = $env:JAVA_HOME
)

$ErrorActionPreference = 'Continue'
$repo = Split-Path -Parent $PSScriptRoot
$engine = Join-Path $repo 'engine'
$dist = Join-Path $engine 'dist\tatami-engine'

if (-not $JavaHome -or -not (Test-Path (Join-Path $JavaHome 'bin\java.exe'))) {
    $JavaHome = Join-Path $env:USERPROFILE 'scoop\apps\temurin21-jdk\current'
}
if (-not (Test-Path (Join-Path $JavaHome 'bin\java.exe'))) {
    throw "JDK introuvable (JAVA_HOME='$JavaHome'). Passez -JavaHome."
}
$env:JAVA_HOME = $JavaHome
$env:Path = "$env:USERPROFILE\scoop\shims;$JavaHome\bin;" + $env:Path
Write-Host "JDK: $JavaHome"
& java -version 2>&1 | Select-Object -First 1 | Out-Host

Push-Location $engine
try {
    # `--console=plain` + `Tee-Object` : la sortie complète va à l'écran **et**
    # dans un journal. Le `Select-Object -Last 3` d'avant avalait tout jusqu'à
    # la fin, donc une exécution bloquée ne disait pas où elle en était —
    # mesuré le 2026-10-20 : 44 min de silence sur le runner avant l'annulation
    # du job, sans un octet pour nommer la dépendance en cause.
    $journal = Join-Path $engine 'build\installDist.log'
    New-Item -ItemType Directory -Force -Path (Split-Path $journal) | Out-Null
    & gradle installDist --console=plain --stacktrace 2>&1 | Tee-Object -FilePath $journal | Out-Host
    if ($LASTEXITCODE -ne 0) {
        Get-Content $journal -Tail 30 | Out-Host
        throw "gradle installDist a échoué (journal : $journal)."
    }
} finally { Pop-Location }

$installLib = Join-Path $engine 'build\install\tatami-engine\lib'
$jars = Get-ChildItem $installLib -Filter '*.jar'
$modules = (& jdeps --multi-release 21 --ignore-missing-deps --print-module-deps $jars.FullName 2>$null | Select-Object -Last 1).Trim()
# jdk.crypto.ec (SunEC, suites ECDHE/TLS) est chargé via les providers de sécurité,
# jdeps ne le voit pas : sans lui, handshake_failure sur les CDN modernes (constaté en mesure).
$modules = "$modules,jdk.crypto.ec"
Write-Host "Modules: $modules"

# Build dans un dossier temoin, echange seulement a la fin : un jlink en echec
# ne doit jamais laisser un dist a moitie supprime (constaté le 2026-10-02).
$stage = Join-Path $engine 'dist\.stage'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
$runtime = Join-Path $stage 'runtime'
& jlink --add-modules $modules --strip-debug --no-header-files --no-man-pages --compress=2 --output $runtime
if ($LASTEXITCODE -ne 0) { throw 'jlink a échoué (dist existant laissé intact).' }

Copy-Item $installLib (Join-Path $stage 'lib') -Recurse
@'
@echo off
setlocal
"%~dp0runtime\bin\java" -Xmx256m -XX:+UseSerialGC -cp "%~dp0lib\*" org.tatami.engine.MainKt %*
'@ | Set-Content (Join-Path $stage 'tatami-engine.bat') -Encoding Ascii

if (Test-Path $dist) {
    try {
        Remove-Item -Recurse -Force $dist -ErrorAction Stop
    } catch {
        throw "Impossible de remplacer $dist (fichiers utilises ?). Fermez l'application puis relancez : le build est pret dans $stage."
    }
}
Move-Item $stage $dist
$mb = [math]::Round(((Get-ChildItem $dist -Recurse -File | Measure-Object Length -Sum).Sum / 1MB), 1)
Write-Host "OK: $dist ($mb MB)"
