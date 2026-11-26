# Vosk (alphacep, Apache 2.0) pour « Clip ça ! » : bibliothèque Windows et modèle français,
# versions officielles vérifiées par SHA-256, dans app/src-tauri/resources/vosk (hors git, livré par
# l'installeur Windows via tauri.windows.conf.json). Ne fait rien si tout est déjà là.
#   pwsh -NoProfile -File scripts/get-vosk.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root "app/src-tauri/resources/vosk"
$files = @(
  @{ Url = "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/vosk-win64-0.3.45.zip"; Sha = "f1dcc9cca460630f81ea8f71794f69c80bed6556d2a4e6237b5785e1d2dff34b"; Check = "libvosk.dll" },
  @{ Url = "https://alphacephei.com/vosk/models/vosk-model-small-fr-0.22.zip"; Sha = "cabf6180e177eb9b3a9a9d43a437bd5e549f3a7d09525e5d69a3fed787be12ad"; Check = "model/am/final.mdl" }
)
if (($files | Where-Object { -not (Test-Path (Join-Path $dest $_.Check)) }).Count -eq 0) {
  Write-Host "Vosk : déjà présent dans $dest"
  exit 0
}
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("iclips-vosk-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force $tmp, $dest | Out-Null
try {
  foreach ($f in $files) {
    $zip = Join-Path $tmp ([IO.Path]::GetFileName($f.Url))
    Write-Host "Vosk : téléchargement de $($f.Url)"
    Invoke-WebRequest -Uri $f.Url -OutFile $zip -UseBasicParsing
    $sha = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
    if ($sha -ne $f.Sha) { throw "Vosk : empreinte inattendue pour $zip ($sha)" }
    $out = Join-Path $tmp ([IO.Path]::GetFileNameWithoutExtension($zip))
    Expand-Archive -Path $zip -DestinationPath $out -Force
    $inner = Get-ChildItem $out -Directory | Select-Object -First 1
    if ($f.Check -eq "libvosk.dll") {
      Get-ChildItem $inner.FullName -Filter *.dll | Copy-Item -Destination $dest -Force
    } else {
      $model = Join-Path $dest "model"
      if (Test-Path $model) { Remove-Item -Recurse -Force $model }
      Copy-Item -Recurse $inner.FullName $model
    }
  }
  Write-Host "Vosk : prêt dans $dest"
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
