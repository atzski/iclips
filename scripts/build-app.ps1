# Construit l'installeur de l'application autonome (app/) sans y laisser de chemins personnels.
#
#   pwsh -NoProfile -File scripts/build-app.ps1
#
# Comme scripts/build-app.ps1 de Music in the Box et scripts/release.ps1 d'iBox :
#   - les chemins de compilation (nom d'utilisateur Windows, dossiers cargo et rustup) sont
#     remplacés dans le binaire (--remap-path-prefix) ;
#   - le programme, l'installeur et l'interface compilée sont ensuite analysés : nom
#     d'utilisateur, dossier personnel, nom du PC. Une seule trace arrête tout ;
#   - l'installeur vérifié est copié dans dist/, prêt pour `gh release create`.
# L'interface est celle du plugin (ui/, construite dans dist/ui par `npm run build`).
param([switch]$SkipTests)
$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$app = Join-Path $root "app"
$version = (Get-Content -LiteralPath (Join-Path $app "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
$name = "iClips_${version}_x64-setup.exe"

function Assert-NoLeak([string[]]$files) {
  $latin1 = [Text.Encoding]::GetEncoding(28591)
  $needles = @($env:USERNAME, $env:USERPROFILE, $env:COMPUTERNAME) | Where-Object { $_ -and $_.Length -ge 4 } | Select-Object -Unique
  foreach ($file in $files) {
    $text = $latin1.GetString([IO.File]::ReadAllBytes($file))
    foreach ($needle in $needles) {
      foreach ($form in @($needle, $latin1.GetString([Text.Encoding]::Unicode.GetBytes($needle)), $latin1.GetString([Text.Encoding]::UTF8.GetBytes($needle)))) {
        if ($text.IndexOf($form, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
          $count = ([regex]::Matches($text, [regex]::Escape($form), "IgnoreCase")).Count
          throw "Information personnelle trouvée dans $(Split-Path -Leaf $file) ($count occurrence(s)). Rien n'est publié."
        }
      }
    }
  }
  Write-Host "Analyse : aucune information personnelle dans $($files.Count) fichier(s)."
}

Push-Location $root
try {
  if (-not $SkipTests) {
    npm test
    if ($LASTEXITCODE -ne 0) { throw "Tests de l'interface : échec. Rien n'est construit." }
    Push-Location (Join-Path $app "src-tauri")
    try {
      cargo test --lib
      if ($LASTEXITCODE -ne 0) { throw "Tests natifs : échec. Rien n'est construit." }
    } finally { Pop-Location }
  }

  if (-not (Test-Path (Join-Path $app "node_modules"))) {
    Push-Location $app
    try { npm install --no-audit --no-fund; if ($LASTEXITCODE -ne 0) { throw "npm install (app) : échec." } } finally { Pop-Location }
  }

  $saved = @{}
  foreach ($var in "CARGO_ENCODED_RUSTFLAGS", "CARGO_TARGET_DIR") { $saved[$var] = [Environment]::GetEnvironmentVariable($var) }
  $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
  $rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { "$env:USERPROFILE\.rustup" }
  # La dernière règle qui correspond l'emporte : de la plus générale à la plus précise.
  $remaps = @(
    "--remap-path-prefix=$env:USERPROFILE=C:\build",
    "--remap-path-prefix=$cargoHome=C:\cargo",
    "--remap-path-prefix=$rustupHome=C:\rustup",
    "--remap-path-prefix=$root=C:\iclips"
  )
  # dossier séparé : la construction de développement (chemins réels) n'est jamais publiée
  $target = Join-Path $app "src-tauri\target\publish"
  Push-Location $app
  try {
    $env:CARGO_ENCODED_RUSTFLAGS = $remaps -join [char]0x1f
    $env:CARGO_TARGET_DIR = $target
    # Vosk (« Clip ça ! ») livré par l'installeur : versions officielles vérifiées par SHA-256
    & (Join-Path $PSScriptRoot "get-vosk.ps1")
    if (-not $?) { throw "Vosk indisponible : impossible de construire l'installeur." }
    npx tauri build --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "Échec de la compilation Tauri (code $LASTEXITCODE)." }
  } finally {
    Pop-Location
    foreach ($var in $saved.Keys) { [Environment]::SetEnvironmentVariable($var, $saved[$var]) }
  }

  $exe = Join-Path $target "release\iclips.exe"
  $installer = Join-Path $target "release\bundle\nsis\$name"
  # L'interface (dist/ui) est compressée dans le programme : on l'analyse avant compression.
  $front = @(Get-ChildItem -LiteralPath (Join-Path $root "dist\ui") -Recurse -File -Include *.js, *.html, *.css, *.json, *.svg | ForEach-Object FullName)
  Assert-NoLeak (@($exe, $installer) + $front)

  $out = Join-Path $root "dist"
  New-Item -ItemType Directory -Force $out | Out-Null
  Copy-Item -LiteralPath $installer -Destination $out -Force
  $hash = (Get-FileHash -LiteralPath (Join-Path $out $name) -Algorithm SHA256).Hash.ToLowerInvariant()
  Write-Host "Installeur : $(Join-Path $out $name)"
  Write-Host "sha256 $hash"
} finally { Pop-Location }
