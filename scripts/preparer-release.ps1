#Requires -Version 5.1
<#
Prépare les fichiers d'une release Tatami : `latest.json` et `SHA256SUMS.txt`.

Ce script ne construit rien et ne publie rien. Il prend les artefacts que
`tauri build` vient d'écrire, refuse une sortie incomplète, et produit les deux
fichiers que le workflow de release envoie sur GitHub.

Pourquoi un script plutôt que des lignes dans le YAML : la même préparation
doit pouvoir être rejouée à la main (M5, poste de test), et une étape YAML de
trente lignes ne se teste pas hors CI.

Ce qui est vérifié ici, et ce qui ne l'est pas :

- vérifié : la version demandée est celle des noms de fichiers (une sortie de
  build périmée est le piège classique : `target/` n'est jamais nettoyé) ;
  chaque installeur a une signature non vide, de forme minisign ; le
  manifeste cite l'installeur NSIS et **le contenu exact** de son `.sig`.
- **pas** vérifié ici : la correspondance cryptographique entre la signature et
  l'octet près. C'est le test Rust `signature_installeur`, qui utilise la même
  crate que le plugin (`minisign-verify`) et les mêmes étapes que
  `verify_signature`. Le workflow le lance juste après ce script.

Sortie : `-Sortie` contient l'installeur NSIS et sa `.sig`, le MSI et sa `.sig`
quand il a pu être construit (une prérelease non numérique ne peut pas l'être,
cf. plus bas), `latest.json` et `SHA256SUMS.txt`.

.EXAMPLE
  .\scripts\preparer-release.ps1 -Version 0.2.0-rc.1 -Tag v0.2.0-rc.1 `
      -Depot kevsi/tatami -Notes "Build de test"
#>
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Tag,
    [Parameter(Mandatory)][string]$Depot,
    [string]$BundleDir = 'apps\desktop\src-tauri\target\release\bundle',
    [string]$Sortie = 'dist-release',
    [string]$Notes = ''
)

$ErrorActionPreference = 'Stop'

function Echec([string]$message) {
    Write-Error $message
    exit 1
}

# --- Le tag et la version disent la même chose -----------------------------

if ($Tag -ne "v$Version") {
    Echec "Tag '$Tag' et version '$Version' ne concordent pas : le tag doit être 'v$Version'."
}
if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    Echec "Version '$Version' n'est pas une version sémantique que le plugin sait comparer."
}
if ($Depot -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    Echec "Dépôt '$Depot' n'a pas la forme propriétaire/nom."
}

# --- Les artefacts existent, une seule fois chacun --------------------------
#
# WiX refuse un identifiant de prérelease non numérique : `0.2.0-rc.1` ne peut
# donc **pas** produire de MSI (mesuré le 2026-10-22, run 39785715116 :
# « optional pre-release identifier in app version must be numeric-only »). Une
# version stable exige les deux installeurs ; une prérelease n'en exige qu'un,
# et `latest.json` ne cite de toute façon que le NSIS : c'est lui que
# `tauri-plugin-updater` installe sous Windows.

$estPrerelease = $Version.Contains('-')

$nsis = @(Get-ChildItem (Join-Path $BundleDir 'nsis\*-setup.exe') -ErrorAction SilentlyContinue)
$msi = @(Get-ChildItem (Join-Path $BundleDir 'msi\*.msi') -ErrorAction SilentlyContinue)

if ($nsis.Count -ne 1) {
    Echec "Attendu un installeur NSIS dans '$BundleDir\nsis', trouvé $($nsis.Count). Lancer 'pnpm tauri build --config src-tauri/tauri.test.conf.json'."
}
if ($msi.Count -gt 1) {
    Echec "Attendu au plus un installeur MSI dans '$BundleDir\msi', trouvé $($msi.Count)."
}
if (-not $estPrerelease -and $msi.Count -ne 1) {
    Echec "Attendu un installeur MSI dans '$BundleDir\msi' pour une version stable, trouvé $($msi.Count)."
}

$installateurs = @($nsis[0])
if ($msi.Count -eq 1) { $installateurs += $msi[0] }

foreach ($f in $installateurs) {
    if ($f.Name -notlike "*$Version*") {
        Echec "L'artefact '$($f.Name)' ne porte pas la version '$Version' : la sortie de build est périmée (target/ n'est pas nettoyé entre deux versions)."
    }
}

# --- La signature a la forme attendue --------------------------------------
#
# Tauri écrit dans `<fichier>.sig` la **base64** du fichier minisign, et c'est
# ce texte que le manifeste doit porter (`verify_signature` le décode avec
# `base64_to_string`, puis `Signature::decode`). On vérifie donc la forme ici,
# sans prétendre vérifier la cryptographie.

function Lire-Signature([System.IO.FileInfo]$fichier) {
    $chemin = "$($fichier.FullName).sig"
    if (-not (Test-Path $chemin)) {
        Echec "Signature absente : '$chemin'. Un build sans TAURI_SIGNING_PRIVATE_KEY produit des installeurs non signés, que l'auto-update refusera."
    }
    $texte = (Get-Content $chemin -Raw).Trim()
    if ([string]::IsNullOrWhiteSpace($texte)) {
        Echec "Signature vide : '$chemin'."
    }
    try {
        $octets = [Convert]::FromBase64String($texte)
    } catch {
        Echec "Signature '$chemin' n'est pas de la base64 : $($_.Exception.Message)"
    }
    $clair = [Text.Encoding]::UTF8.GetString($octets)
    if (-not $clair.StartsWith('untrusted comment:')) {
        Echec "Signature '$chemin' ne contient pas un fichier minisign (attendu : 'untrusted comment:')."
    }
    $lignes = @($clair -split "`n" | Where-Object { $_.Trim() -ne '' })
    if ($lignes.Count -lt 4) {
        Echec "Signature '$chemin' tronquée : $($lignes.Count) ligne(s) au lieu de 4."
    }
    return $texte
}

$signatureNsis = Lire-Signature $nsis[0]
if ($msi.Count -eq 1) {
    # On exige une signature valide quand le MSI existe : un installeur publié
    # sans `.sig` serait un fichier que personne ne peut vérifier.
    $null = Lire-Signature $msi[0]
}

# --- latest.json -----------------------------------------------------------
#
# Structure attendue par `tauri-plugin-updater` 2.11.0 (`RemoteRelease` +
# `ReleaseManifestPlatform`), vérifiée dans le code du plugin et non devinée :
#   version (semver), notes (optionnel), pub_date (RFC 3339), platforms.<cible>
#   = { url, signature }.
# La cible Windows x64 est `windows-x86_64`.
#
# L'URL pointe la prérelease **du tag** : `releases/latest` n'est jamais servi
# par une prérelease, donc une release de test ne peut pas être installée par
# un utilisateur stable.

$urlNsis = "https://github.com/$Depot/releases/download/$Tag/$($nsis[0].Name)"
$manifeste = [ordered]@{
    version   = $Version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = $signatureNsis
            url       = $urlNsis
        }
    }
}

# --- Écriture --------------------------------------------------------------
#
# UTF-8 **sans BOM** : un BOM dans un JSON fait échouer `JSON.parse` côté JS et
# `serde_json` côté Rust, avec un message qui ne parle pas du BOM.

if (Test-Path $Sortie) {
    Remove-Item $Sortie -Recurse -Force
}
New-Item -ItemType Directory -Path $Sortie -Force | Out-Null

$encoder = New-Object System.Text.UTF8Encoding($false)

foreach ($f in $installateurs) {
    Copy-Item $f.FullName (Join-Path $Sortie $f.Name) -Force
    Copy-Item "$($f.FullName).sig" (Join-Path $Sortie "$($f.Name).sig") -Force
}

$cheminManifeste = Join-Path (Resolve-Path $Sortie).Path 'latest.json'
$json = $manifeste | ConvertTo-Json -Depth 6
[IO.File]::WriteAllText($cheminManifeste, $json, $encoder)

# --- Sommes de contrôle ----------------------------------------------------

$fichiers = @(Get-ChildItem $Sortie -File | Sort-Object Name)
$lignes = foreach ($f in $fichiers) {
    $hash = (Get-FileHash $f.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($f.Name)"
}
$cheminSommes = Join-Path (Resolve-Path $Sortie).Path 'SHA256SUMS.txt'
[IO.File]::WriteAllText($cheminSommes, (($lignes -join "`r`n") + "`r`n"), $encoder)

# --- Récapitulatif ---------------------------------------------------------

Write-Host "Release préparée : $Tag ($Version), dépôt $Depot"
Write-Host "  NSIS  : $($nsis[0].Name) ($([math]::Round($nsis[0].Length / 1MB, 1)) Mo)"
if ($msi.Count -eq 1) {
    Write-Host "  MSI   : $($msi[0].Name) ($([math]::Round($msi[0].Length / 1MB, 1)) Mo)"
} else {
    Write-Host "  MSI   : absent (WiX exige un identifiant de prérelease numérique, donc une prérelease est NSIS seule)."
}
Write-Host "  URL   : $urlNsis"
Write-Host "  Sortie: $((Resolve-Path $Sortie).Path)"
Get-ChildItem $Sortie -File | ForEach-Object { Write-Host "    $($_.Name) ($($_.Length) o)" }
