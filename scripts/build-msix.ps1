# build-msix.ps1
# ---------------------------------------------------------------------------
# Emballe la sortie de `pnpm tauri build` en paquet .msix.
#
# Prerequis :
#   - Windows SDK 10 (makeappx.exe + signtool.exe) installes (cf. chemin ci-dessous).
#   - Un certificat de signature de code (pour test local). Pour le Store,
#     Microsoft signe apres certification : le paquet peut rester non signe,
#     mais Windows refuse l'installation locale d'un .msix non signe.
#   - Build Tauri deja fait : apps\desktop\src-tauri\target\release\bundle\...
#
# Usage :
#   .\scripts\build-msix.ps1
#   .\scripts\build-msix.ps1 -Version "0.2.0.7" -Publisher "CN=MonNom" -Sign -PfxPath "C:\...\cert.pfx" -PfxPassword "..."
#   .\scripts\build-msix.ps1 -Version "0.2.0.7" -Publisher "CN=MonNom" -SkipSign   # pour soumission Store
# ---------------------------------------------------------------------------
param(
  [string]$Version = "0.2.0.0",
  [string]$Publisher = "CN=TatamiDev",
  [string]$PublisherDisplay = "Tatami",
  [string]$Arch = "x64",
  [string]$PfxPath = "",
  [string]$PfxPassword = "",
  [switch]$Sign,
  [switch]$SkipSign,
  [switch]$RunWack
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$DesktopDir = Join-Path $RepoRoot "apps\desktop"
$TauriTarget = Join-Path $DesktopDir "src-tauri\target\release"
$BundleDir = Join-Path $TauriTarget "bundle"
$MsixDir = Join-Path $PSScriptRoot "msix"
$LayoutDir = Join-Path $MsixDir "layout"
$ManifestTemplate = Join-Path $MsixDir "AppxManifest.xml"
$OutputMsix = Join-Path $MsixDir "Tatami-$Version.msix"

# --- Localiser les outils du SDK Windows ---
$SdkBin = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\$Arch"
$MakeAppx = Join-Path $SdkBin "makeappx.exe"
$SignTool = Join-Path $SdkBin "signtool.exe"
$AppCert = "C:\Program Files (x86)\Windows Kits\10\App Certification Kit\appcert.exe"

foreach ($tool in @($MakeAppx)) {
  if (-not (Test-Path $tool)) { throw "Outil introuvable : $tool" }
}

# --- Verifier que le build Tauri existe ---
$ExePath = Join-Path $TauriTarget "tatami-desktop.exe"
if (-not (Test-Path $ExePath)) {
  throw "Binaire Tauri introuvable : $ExePath. Lance d'abord 'pnpm tauri build'."
}

# --- Preparer la mise en page ---
if (Test-Path $LayoutDir) { Remove-Item -Recurse -Force $LayoutDir }
New-Item -ItemType Directory -Path $LayoutDir | Out-Null
New-Item -ItemType Directory -Path (Join-Path $LayoutDir "Assets") | Out-Null

# Copier le binaire et les ressources bundlees par Tauri.
Copy-Item $ExePath (Join-Path $LayoutDir "Tatami.exe") -Force
$ResourcesSrc = Join-Path $TauriTarget "resources"
if (Test-Path $ResourcesSrc) {
  Copy-Item -Recurse -Force (Join-Path $ResourcesSrc "*") $LayoutDir
}
# Moteur JVM (sidecar) : a copier depuis engine/dist/tatami-engine si present.
$EngineSrc = Join-Path $RepoRoot "engine\dist\tatami-engine"
if (Test-Path $EngineSrc) {
  $EngineDst = Join-Path $LayoutDir "engine\tatami-engine"
  New-Item -ItemType Directory -Path $EngineDst -Force | Out-Null
  Copy-Item -Recurse -Force (Join-Path $EngineSrc "*") $EngineDst
}
# Sources personnalisees (resources/sources).
$SourcesSrc = Join-Path $DesktopDir "src-tauri\resources\sources"
if (Test-Path $SourcesSrc) {
  New-Item -ItemType Directory -Path (Join-Path $LayoutDir "sources") -Force | Out-Null
  Copy-Item -Recurse -Force (Join-Path $SourcesSrc "*") (Join-Path $LayoutDir "sources")
}

# Copier les logos Store depuis src-tauri/icons (noms attendus par le manifest).
$IconsDir = Join-Path $DesktopDir "src-tauri\icons"
foreach ($asset in @("StoreLogo.png","Square44x44Logo.png","Square150x150Logo.png","Square310x310Logo.png")) {
  $src = Join-Path $IconsDir $asset
  if (Test-Path $src) { Copy-Item $src (Join-Path $LayoutDir "Assets\$asset") -Force }
}

# --- Generer le manifest depuis le template ---
$manifest = Get-Content $ManifestTemplate -Raw
$manifest = $manifest.Replace("__PUBLISHER__", $Publisher)
$manifest = $manifest.Replace("__PUBLISHER_DISPLAY__", $PublisherDisplay)
$manifest = $manifest.Replace("__VERSION__", $Version)
$manifest | Set-Content -Path (Join-Path $LayoutDir "AppxManifest.xml") -Encoding UTF8

# --- Empaqueter ---
if (Test-Path $OutputMsix) { Remove-Item $OutputMsix -Force }
& $MakeAppx pack /d $LayoutDir /p $OutputMsix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx a echoue (code $LASTEXITCODE)" }
Write-Host "Paquet cree : $OutputMsix" -ForegroundColor Green

# --- Signer (optionnel) ---
if ($Sign -and -not $SkipSign) {
  if ([string]::IsNullOrWhiteSpace($PfxPath)) {
    throw "Signature demandee mais -PfxPath manquant."
  }
  $sigArgs = @("sign", "/fd", "SHA256", "/f", $PfxPath)
  if (-not [string]::IsNullOrWhiteSpace($PfxPassword)) {
    $sigArgs += @("/p", $PfxPassword)
  }
  $sigArgs += @("/tr", "http://timestamp.digicert.com", "/td", "SHA256", $OutputMsix)
  & $SignTool @sigArgs
  if ($LASTEXITCODE -ne 0) { throw "signtool a echoue (code $LASTEXITCODE)" }
  Write-Host "Paquet signe." -ForegroundColor Green
} elseif ($SkipSign) {
  Write-Host "Signature ignoree (-SkipSign) : paquet pret pour soumission Store." -ForegroundColor Yellow
} else {
  Write-Host "Non signe : pour installer localement, utilise -Sign ou -SkipSign pour le Store." -ForegroundColor Yellow
}

# --- Validation WACK (optionnelle) ---
if ($RunWack -and (Test-Path $AppCert)) {
  $wackReport = Join-Path $MsixDir "wack-report.xml"
  & $AppCert test -appxpackagepath $OutputMsix -reportoutputpath $wackReport
  Write-Host "Rapport WACK : $wackReport"
}

Write-Host "Termine." -ForegroundColor Cyan
