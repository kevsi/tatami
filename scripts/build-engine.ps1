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
    # La sortie de Gradle va dans un **fichier**, via `cmd /c`, et non par un
    # pipeline PowerShell. Mesuré le 2026-10-20 (run 39437468129) : le build
    # s'est terminé — « BUILD SUCCESSFUL in 2m 47s » écrit dans le journal — et
    # le script est resté bloqué **17 min de plus** parce que
    # `Tee-Object | Out-Host` attend la fin du flux, qu'un processus enfant
    # (le démon Gradle) tenait encore ouvert. Le job a été annulé à son délai
    # alors que le travail était fait. `cmd /c` rend la main quand le client
    # Gradle sort, et un fichier n'a pas de fin de flux à attendre.
    $journal = Join-Path $engine 'build\installDist.log'
    New-Item -ItemType Directory -Force -Path (Split-Path $journal) | Out-Null
    Write-Host "gradle installDist : sortie dans $journal"
    $ligne = 'gradle installDist --console=plain --stacktrace > "{0}" 2>&1' -f $journal
    cmd /c $ligne
    if ($LASTEXITCODE -ne 0) {
        Get-Content $journal -Tail 40 | Out-Host
        throw "gradle installDist a échoué (journal : $journal)."
    }
    Get-Content $journal -Tail 4 | Out-Host
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
