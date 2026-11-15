param(
  [string]$Version = "",
  [string]$Prefix = "",
  [switch]$NoStart,
  [string]$ApiUrl = "https://api.github.com/repos/beyondcy1013/webClx/releases"
)

$ErrorActionPreference = "Stop"

if (-not $Prefix) {
  $Prefix = Join-Path $env:LOCALAPPDATA "webClx"
}

foreach ($command in "curl.exe", "tar.exe", "powershell.exe") {
  if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
    throw "webClx installer requires $command"
  }
}

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
  "AMD64" { "x86_64"; break }
  "ARM64" { "arm64"; break }
  default { throw "unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}
$platform = "windows-$arch"
$releasePath = if ($Version) { "/tags/v$($Version.TrimStart('v'))" } else { "/latest" }
$releaseJson = & curl.exe -fsSL "$ApiUrl$releasePath" | ConvertFrom-Json
$Version = $releaseJson.tag_name.TrimStart("v")
$packageName = "webClx-$Version-$platform.zip"
$archive = Join-Path ([System.IO.Path]::GetTempPath()) "$([System.IO.Path]::GetRandomFileName()).webclx.zip"
$checksum = "$archive.sha256"
$asset = $releaseJson.assets | Where-Object { $_.name -eq $packageName } | Select-Object -First 1
$checksumAsset = $releaseJson.assets | Where-Object { $_.name -eq "$packageName.sha256" } | Select-Object -First 1
if (-not $asset -or -not $checksumAsset) {
  throw "release v$Version does not contain required asset: $packageName(.sha256)"
}

& curl.exe -fsSL --retry 3 -o $archive $asset.browser_download_url
& curl.exe -fsSL --retry 3 -o $checksum $checksumAsset.browser_download_url
$expected = ((Get-Content $checksum -TotalCount 1) -split "\s+")[0]
if ($expected -notmatch '^[0-9A-Fa-f]{64}$') {
  throw "malformed SHA-256 checksum file for $packageName"
}
$actual = (Get-FileHash $archive -Algorithm SHA256).Hash
if ($actual -ne $expected) {
  throw "SHA-256 mismatch for $packageName"
}

$installRoot = New-Item -ItemType Directory -Force -Path $Prefix
$versionDir = Join-Path $installRoot.FullName "webclx-$Version"
$stage = Join-Path $installRoot.FullName ".webclx-$Version.next.$PID"
New-Item -ItemType Directory -Path $stage | Out-Null
& tar.exe -xf $archive -C $stage
$packageDir = Join-Path $stage "webClx-$Version-$platform"
if (-not (Test-Path (Join-Path $packageDir "webclx.exe")) -or -not (Test-Path (Join-Path $packageDir "static\index.html"))) {
  throw "release package layout is invalid"
}
if (Test-Path $versionDir) {
  $backup = Join-Path $installRoot.FullName ".webclx-$Version.previous.$PID"
  Move-Item $versionDir $backup
}
Move-Item $packageDir $versionDir

if ($NoStart) {
  Write-Host "Installed webClx $Version at $versionDir"
  Write-Host "Run: cd `"$versionDir`"; .\webclx.exe serve"
} else {
  Write-Host "Starting webClx. Open http://127.0.0.1:11111 when it is ready."
  Write-Host "The first-run password is stored in .webclx-initial-password under the process working directory."
  Set-Location $versionDir
  & .\webclx.exe serve
}
