# build_bacup.ps1 — Build B.A.C.U.P. (Bethesda Asset Converter Universal
# Platform) as a PyInstaller onefile EXE plus the Tales From Appalachia companion
# runtime payload.
#
# Output: ..\dist\BACUP.exe plus ..\dist\mods\B21_TalesFromAppalachia\.
# User data (settings, extracted/, mods/, logs/) is created next to the EXE on
# first run. For the multi-variant onedir release pipeline use build_toolkit.ps1.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File build_bacup.ps1
#   powershell -ExecutionPolicy Bypass -File build_bacup.ps1 -OneDir   # folder build
#
# Requires: Python 3.12+, uv

param(
    [switch]$OneDir
)

$ErrorActionPreference = "Stop"

$BacupRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $BacupRoot
$Version = (Get-Content (Join-Path $RepoRoot "VERSION") -Raw).Trim()

$ExeName  = "BACUP"
$Folder   = "BACUP"
$Icon     = Join-Path $RepoRoot "resource\icons\modbox21-converter.ico"
$CompanionModName = "B21_TalesFromAppalachia"
$GeneratedModNames = @(
    "SeventySix",
    "FNV_FO3_Merged",
    "MojaveCapital",
    "Skyrim_Merged",
    "Skyrim"
)
$SpecFile = Join-Path $BacupRoot "BACUP.spec"
$WorkPath = Join-Path $RepoRoot "build\bacup"
$OneFile  = -not $OneDir

function Assert-NoBundledGameAssets($Root) {
    $forbidden = @(
        (Join-Path $Root "mods\$CompanionModName\PrismaUI_F4\views\B21_FullScreenMap\maps\appalachia\map.jpg"),
        (Join-Path $Root "mods\$CompanionModName\web\maps\appalachia\map.jpg")
    )
    foreach ($generatedModName in $GeneratedModNames) {
        $forbidden += Join-Path $Root "mods\$generatedModName"
    }
    foreach ($path in $forbidden) {
        if (Test-Path $path) {
            throw "Release payload includes generated/game asset path: $path"
        }
    }
}

function Warn-IgnoredSteamworksSourceFiles {
    foreach ($path in @(
        (Join-Path $RepoRoot "steam_appid.txt"),
        (Join-Path $RepoRoot "resource\steam_appid.txt"),
        (Join-Path $RepoRoot "resource\steam_api64.dll")
    )) {
        if (Test-Path $path) {
            Write-Warning "Ignoring local Steamworks runtime/dev file during release build: $path"
        }
    }
}

function Remove-SteamworksPayloadFiles($Root) {
    foreach ($path in @(
        (Join-Path $Root "steam_appid.txt"),
        (Join-Path $Root "resource\steam_appid.txt"),
        (Join-Path $Root "resource\steam_api64.dll"),
        (Join-Path $Root "_internal\resource\steam_appid.txt"),
        (Join-Path $Root "_internal\resource\steam_api64.dll")
    )) {
        if (Test-Path $path) {
            Remove-Item -Force -LiteralPath $path
        }
    }
}

function Assert-NoSteamworksPayloadFiles($Root) {
    foreach ($path in @(
        (Join-Path $Root "steam_appid.txt"),
        (Join-Path $Root "resource\steam_appid.txt"),
        (Join-Path $Root "resource\steam_api64.dll"),
        (Join-Path $Root "_internal\resource\steam_appid.txt"),
        (Join-Path $Root "_internal\resource\steam_api64.dll")
    )) {
        if (Test-Path $path) {
            throw "Release payload includes Steamworks runtime/dev file: $path"
        }
    }
}

function Assert-NoDeveloperPayload($Root) {
    foreach ($name in @("tools", "utils")) {
        $path = Join-Path $Root $name
        if (Test-Path $path) {
            throw "Release payload includes developer directory: $path"
        }
    }
}

function Remove-TreeIfPresent($Path) {
    if (-not (Test-Path $Path)) {
        return
    }
    try {
        Remove-Item -Recurse -Force -LiteralPath $Path -ErrorAction Stop
        return
    } catch {
        Write-Warning "Standard cleanup failed for $Path; retrying bottom-up."
    }
    Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction SilentlyContinue |
        ForEach-Object {
            $_.IsReadOnly = $false
            Remove-Item -Force -LiteralPath $_.FullName -ErrorAction Stop
        }
    Get-ChildItem -LiteralPath $Path -Recurse -Force -Directory -ErrorAction SilentlyContinue |
        Sort-Object { $_.FullName.Length } -Descending |
        ForEach-Object {
            Remove-Item -Force -LiteralPath $_.FullName -ErrorAction Stop
        }
    Remove-Item -Force -LiteralPath $Path -ErrorAction Stop
}

function Copy-AppalachiaCompanionMod($DestinationRoot) {
    $src = Join-Path $RepoRoot "mods\$CompanionModName"
    if (-not (Test-Path $src)) {
        throw "Companion mod not found: $src"
    }

    $dst = Join-Path $DestinationRoot "mods\$CompanionModName"
    if (Test-Path $dst) {
        Remove-Item -Recurse -Force $dst
    }
    New-Item -ItemType Directory -Force -Path $dst | Out-Null

    Copy-Item -Force -LiteralPath (Join-Path $src "$CompanionModName.esp") -Destination (Join-Path $dst "$CompanionModName.esp")

    $runtimeDirs = @("data", "PrismaUI_F4")
    foreach ($dir in $runtimeDirs) {
        $srcDir = Join-Path $src $dir
        if (-not (Test-Path $srcDir)) {
            throw "Companion runtime dir not found: $srcDir"
        }
        $dstDir = Join-Path $dst $dir
        New-Item -ItemType Directory -Force -Path $dstDir | Out-Null
        Get-ChildItem -LiteralPath $srcDir -Recurse -File |
            Where-Object { $_.Extension -ine ".pdb" } |
            ForEach-Object {
                $rel = $_.FullName.Substring($srcDir.Length).TrimStart("\")
                $out = Join-Path $dstDir $rel
                New-Item -ItemType Directory -Force -Path (Split-Path -Parent $out) | Out-Null
                Copy-Item -Force -LiteralPath $_.FullName -Destination $out
            }
    }

    Write-Host "  Copied: mods/$CompanionModName runtime payload"
}

Write-Host "=== B.A.C.U.P. - Bethesda Asset Converter Universal Platform (v$Version, $(if ($OneFile) {'single-file'} else {'folder'})) ===" -ForegroundColor Cyan
Warn-IgnoredSteamworksSourceFiles

# Step 1: Clean only this variant's previous output
Write-Host "`n[1/4] Cleaning previous build..." -ForegroundColor Yellow
foreach ($p in @(
    (Join-Path $RepoRoot "dist\$Folder"),
    (Join-Path $RepoRoot "dist\$ExeName.exe"),
    (Join-Path $RepoRoot "dist\mods\$CompanionModName"),
    $WorkPath
)) {
    Remove-TreeIfPresent $p
}
foreach ($generatedModName in $GeneratedModNames) {
    Remove-TreeIfPresent (Join-Path $RepoRoot "dist\mods\$generatedModName")
}

# Step 2: Ensure the BACUP native extension matches its complete Rust/input
# hash before PyInstaller collects it. A clean PyInstaller build does not check
# Rust and can otherwise package a stale bacup_lib._native.pyd.
Write-Host "`n[2/5] Ensuring BACUP native extension is current..." -ForegroundColor Yellow
$EnsureNativeScript = Join-Path $RepoRoot "scripts\ensure_native.py"
& uv run python $EnsureNativeScript --package bacup
if ($LASTEXITCODE -ne 0) {
    Write-Error "BACUP native ensure failed with exit code $LASTEXITCODE"
    exit 1
}

$InstalledNative = Join-Path $BacupRoot "py_bacup_lib\python\bacup_lib\_native.pyd"
$BuiltNative = Join-Path $BacupRoot "py_bacup_lib\target\maturin\_native.dll"
if (-not (Test-Path $InstalledNative) -or -not (Test-Path $BuiltNative)) {
    throw "BACUP native ensure did not find both installed and staged native binaries."
}
if ((Get-FileHash -Algorithm SHA256 $InstalledNative).Hash -ne (Get-FileHash -Algorithm SHA256 $BuiltNative).Hash) {
    throw "Installed bacup_lib._native.pyd does not match the freshly built native DLL."
}

# Step 3: Run PyInstaller. Exe-name/icon/onefile are selected via env vars (the
# spec reads them); MODBOX21_ONEFILE folds binaries + resource into one EXE.
Write-Host "`n[3/5] Running PyInstaller..." -ForegroundColor Yellow
$env:MODBOX21_EXE_NAME  = $ExeName
$env:MODBOX21_DIST_NAME = $Folder
$env:MODBOX21_ICON      = $Icon
if ($OneFile) { $env:MODBOX21_ONEFILE = "1" } else { Remove-Item Env:\MODBOX21_ONEFILE -ErrorAction SilentlyContinue }
try {
    Push-Location $RepoRoot
    try {
        & uv run --with pyinstaller pyinstaller $SpecFile --noconfirm --workpath $WorkPath
        if ($LASTEXITCODE -ne 0) {
            Write-Error "PyInstaller failed for $ExeName with exit code $LASTEXITCODE"
            exit 1
        }
    } finally {
        Pop-Location
    }
} finally {
    Remove-Item Env:\MODBOX21_EXE_NAME  -ErrorAction SilentlyContinue
    Remove-Item Env:\MODBOX21_DIST_NAME -ErrorAction SilentlyContinue
    Remove-Item Env:\MODBOX21_ICON      -ErrorAction SilentlyContinue
    Remove-Item Env:\MODBOX21_ONEFILE   -ErrorAction SilentlyContinue
}

# Step 4: onedir builds need resource/ copied beside the EXE; onefile bundles
# resource/ in the archive and needs nothing copied.
if (-not $OneFile) {
    Write-Host "`n[4/5] Copying runtime payload (onedir)..." -ForegroundColor Yellow
    $DistDir = Join-Path $RepoRoot "dist\$Folder"
    $resourceSrc = Join-Path $RepoRoot "resource"
    $resourceDst = Join-Path $DistDir "_internal\resource"
    if (Test-Path $resourceSrc) {
        if (Test-Path $resourceDst) { Remove-Item -Recurse -Force $resourceDst }
        New-Item -ItemType Directory -Force -Path $resourceDst | Out-Null
        Copy-Item -Recurse -Force "$resourceSrc\*" $resourceDst
        $spriggitDst = Join-Path $resourceDst "spriggit"
        if (Test-Path $spriggitDst) { Remove-Item -Recurse -Force $spriggitDst }
        Remove-SteamworksPayloadFiles $DistDir
        Write-Host "  Copied: resource/ -> _internal/resource/ (spriggit excluded)"
    }
    Get-ChildItem -Recurse -Include "*.pdb", "*.debug" $DistDir | Remove-Item -Force
} else {
    Write-Host "`n[4/5] Single-file build: resource bundled in the EXE, nothing to copy." -ForegroundColor Yellow
}

$PayloadRoot = if ($OneFile) { Join-Path $RepoRoot "dist" } else { Join-Path $RepoRoot "dist\$Folder" }
if (Test-Path (Join-Path $RepoRoot "mods\$CompanionModName")) {
    Copy-AppalachiaCompanionMod $PayloadRoot
} else {
    Write-Host "  skip companion mod (not present)"
}
Remove-SteamworksPayloadFiles $PayloadRoot
Assert-NoBundledGameAssets $PayloadRoot
Assert-NoSteamworksPayloadFiles $PayloadRoot
if (-not $OneFile) {
    Assert-NoDeveloperPayload $PayloadRoot
}

# Step 5: Report output location
Write-Host "`n[5/5] Done." -ForegroundColor Yellow
if ($OneFile) {
    $exePath = Join-Path $RepoRoot "dist\$ExeName.exe"
} else {
    $exePath = Join-Path $RepoRoot "dist\$Folder\$ExeName.exe"
}
$size = if (Test-Path $exePath) { "{0:N0} MB" -f ((Get-Item $exePath).Length / 1MB) } else { "?" }
Write-Host "`nEXE: $exePath  ($size)" -ForegroundColor Green
