# build_bacup.ps1 — Build B.A.C.U.P. (Bethesda Asset Converter Universal
# Platform) as a PyInstaller onefile EXE plus the Tales From Appalachia companion
# runtime payload.
#
# Output: ..\dist\BACUP.exe plus ..\dist\mods\B21_TalesFromAppalachia\,
# ..\dist\mods\B21_FullScreenMap\, ..\dist\mods\B21_DevTools\ and
# ..\dist\mods\fallout4-community-shaders\.
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
$BundledMods = @(
    @{ Name = "B21_FullScreenMap"; RootFiles = @(); RuntimeDirs = @("F4SE"); RuntimeFiles = @("data\Scripts\B21_FullScreenMap.pex") },
    # Optional install: BACUP deploys it only when "Install B21 DevTools" is on.
    # Its source ini is a dev working copy; the animation/diagnostic probes hook engine
    # call sites and freeze the game, and the Flies keys mutate Scorchbeast race data,
    # so the shipped copy forces all of them off whatever the source says.
    @{ Name = "B21_DevTools"; RootFiles = @(); RuntimeDirs = @("F4SE"); IniOverrides = @{
        "F4SE\Plugins\B21_DevTools.ini" = [ordered]@{
            DumpConsoleSelectionToLog = "0"; LogAnimationEvents = "0"; LogAttackData = "0"
            LogWeaponGraphs = "0"; LogLocomotion = "0"; PumpShotgunRangeProbe = "0"
            LogCatalogRequests = "0"; FliesToggle = "0"; FliesReport = "0"; FliesActorSwap = "0"
        }
    } }
    # Optional install: BACUP deploys it only when native assets and "Install FO4CS" are on.
    # Its xmake target is FO4CS and it has no Papyrus, plugin or archives.
    @{ Name = "fallout4-community-shaders"; Target = "FO4CS"; NativeOnly = $true; RootFiles = @(); RuntimeDirs = @("F4SE") }
)
$RetiredModNames = @(
    "B21_LegendaryStars"
)
$GeneratedModNames = @(
    "SeventySix",
    "FNV_FO3",
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
        (Join-Path $Root "mods\$CompanionModName\F4SE\Plugins\B21_FullScreenMap\maps\appalachia\map.dds"),
        (Join-Path $Root "mods\$CompanionModName\web\maps\appalachia\map.jpg"),
        (Join-Path $Root "mods\B21_FullScreenMap\F4SE\Plugins\B21_FullScreenMap\maps\appalachia\map.dds"),
        (Join-Path $Root "mods\B21_FullScreenMap\F4SE\Plugins\B21_FullScreenMap\maps\colorized\appalachia.dds")
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

function Get-Sha256Hex($Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($sha256.ComputeHash($stream))).Replace("-", "")
    } finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

function Get-DllPdbReference($DllPath) {
    $reader = New-Object System.IO.BinaryReader([System.IO.File]::OpenRead($DllPath))
    try {
        $stream = $reader.BaseStream
        $stream.Position = 0x3C
        $coff = $reader.ReadInt32() + 4
        $stream.Position = $coff + 2
        $sectionCount = $reader.ReadUInt16()
        $stream.Position = $coff + 16
        $optionalSize = $reader.ReadUInt16()
        $optional = $coff + 20
        # PE32+ only: the debug data directory is entry 6 of the table at +112.
        $stream.Position = $optional + 112 + 6 * 8
        $debugRva = $reader.ReadUInt32()
        $debugSize = $reader.ReadUInt32()
        $debugOffset = $null
        for ($i = 0; $i -lt $sectionCount; $i++) {
            $stream.Position = $optional + $optionalSize + $i * 40 + 8
            $virtualSize = $reader.ReadUInt32()
            $virtualAddress = $reader.ReadUInt32()
            $null = $reader.ReadUInt32()
            $rawPointer = $reader.ReadUInt32()
            if ($debugRva -ge $virtualAddress -and $debugRva -lt $virtualAddress + $virtualSize) {
                $debugOffset = $debugRva - $virtualAddress + $rawPointer
            }
        }
        if ($null -eq $debugOffset) { return $null }
        for ($entry = 0; $entry -lt [int]($debugSize / 28); $entry++) {
            $stream.Position = $debugOffset + $entry * 28 + 12
            if ($reader.ReadUInt32() -ne 2) { continue }
            $dataSize = $reader.ReadUInt32()
            $null = $reader.ReadUInt32()
            $stream.Position = $reader.ReadUInt32()
            if ([Text.Encoding]::ASCII.GetString($reader.ReadBytes(4)) -ne "RSDS") { continue }
            $guid = [BitConverter]::ToString($reader.ReadBytes(16))
            $null = $reader.ReadUInt32()
            $path = [Text.Encoding]::UTF8.GetString($reader.ReadBytes($dataSize - 24)).TrimEnd([char]0)
            return @{ Guid = $guid; Path = $path }
        }
        return $null
    } finally {
        $reader.Dispose()
    }
}

function Get-PdbGuid($PdbPath) {
    $reader = New-Object System.IO.BinaryReader([System.IO.File]::OpenRead($PdbPath))
    try {
        $stream = $reader.BaseStream
        if ([Text.Encoding]::ASCII.GetString($reader.ReadBytes(24)) -ne "Microsoft C/C++ MSF 7.00") {
            throw "Not an MSF 7.00 PDB: $PdbPath"
        }
        $stream.Position = 32
        $blockSize = $reader.ReadUInt32()
        $stream.Position = 44
        $directoryBytes = $reader.ReadUInt32()
        $stream.Position = 52
        $blockMap = $reader.ReadUInt32()
        $directoryBlockCount = [int][Math]::Ceiling($directoryBytes / $blockSize)
        $stream.Position = [int64]$blockMap * $blockSize
        $directoryBlocks = @(for ($i = 0; $i -lt $directoryBlockCount; $i++) { $reader.ReadUInt32() })
        $directory = New-Object System.IO.MemoryStream
        foreach ($block in $directoryBlocks) {
            $stream.Position = [int64]$block * $blockSize
            $chunk = $reader.ReadBytes([int]$blockSize)
            $directory.Write($chunk, 0, $chunk.Length)
        }
        $directory.Position = 0
        $dir = New-Object System.IO.BinaryReader($directory)
        $streamCount = $dir.ReadUInt32()
        $streamZeroSize = $dir.ReadUInt32()
        $streamZeroBlocks = if ($streamZeroSize -eq [uint32]::MaxValue) { 0 } else { [int][Math]::Ceiling($streamZeroSize / $blockSize) }
        # The PDB info stream (1) starts after the remaining sizes and stream 0's block list.
        $directory.Position = 4 + 4 * $streamCount + 4 * $streamZeroBlocks
        $infoBlock = $dir.ReadUInt32()
        $stream.Position = [int64]$infoBlock * $blockSize + 12
        return [BitConverter]::ToString($reader.ReadBytes(16))
    } finally {
        $reader.Dispose()
    }
}

# Full PDBs name every source, object and library by local path. Ship the linker's
# /PDBSTRIPPED copy (public symbols name crash-log frames) with those paths cut to file names.
function Install-CleanPluginPdbs($Root) {
    $cleaner = Join-Path $BacupRoot "scripts\clean_pdb.py"
    foreach ($modDir in Get-ChildItem -LiteralPath (Join-Path $Root "mods") -Directory) {
        $pluginsDir = Join-Path $modDir.FullName "F4SE\Plugins"
        if (-not (Test-Path -LiteralPath $pluginsDir)) { continue }
        foreach ($dll in Get-ChildItem -LiteralPath $pluginsDir -Filter "*.dll" -File) {
            $pdb = [IO.Path]::ChangeExtension($dll.FullName, ".pdb")
            if (-not (Test-Path -LiteralPath $pdb -PathType Leaf)) { continue }
            # /PDBSTRIPPED writes to $(builddir): build\ or a session's build\<name>\.
            $buildRoot = Join-Path $RepoRoot "mods\$($modDir.Name)\build"
            $strippedName = "$($dll.BaseName).stripped.pdb"
            $guid = (Get-DllPdbReference $dll.FullName).Guid
            $stripped = @(
                (Join-Path $buildRoot $strippedName)
                Get-ChildItem -LiteralPath $buildRoot -Directory -ErrorAction SilentlyContinue |
                    ForEach-Object { Join-Path $_.FullName $strippedName }
            ) | Where-Object { (Test-Path -LiteralPath $_ -PathType Leaf) -and (Get-PdbGuid $_) -eq $guid } |
                Select-Object -First 1
            if (-not $stripped) {
                throw "No stripped PDB matching $($dll.Name) under $buildRoot. Rebuild $($modDir.Name) (its link writes $strippedName via /PDBSTRIPPED)."
            }
            & uv run python $cleaner $stripped $pdb
            if ($LASTEXITCODE -ne 0) { throw "PDB cleaning failed for $stripped ($LASTEXITCODE)" }
        }
    }
}

# A shipped .pdb must be the one linked with its DLL and must not leak build paths;
# a stale one mis-symbolicates user crash reports.
function Assert-CleanPluginPdbs($Root) {
    $pluginDirs = Get-ChildItem -LiteralPath (Join-Path $Root "mods") -Directory |
        ForEach-Object { Join-Path $_.FullName "F4SE\Plugins" } |
        Where-Object { Test-Path -LiteralPath $_ }
    foreach ($pluginsDir in $pluginDirs) {
        foreach ($dll in Get-ChildItem -LiteralPath $pluginsDir -Filter "*.dll" -File) {
            $pdb = [IO.Path]::ChangeExtension($dll.FullName, ".pdb")
            if (-not (Test-Path -LiteralPath $pdb -PathType Leaf)) { continue }
            $reference = Get-DllPdbReference $dll.FullName
            if ($null -eq $reference) {
                throw "Shipped PDB has no CodeView record in its DLL: $($dll.FullName)"
            }
            $pdbName = Split-Path -Leaf $pdb
            if ($reference.Path -ne $pdbName) {
                throw "DLL embeds a build path instead of '$pdbName': $($dll.FullName) -> $($reference.Path)"
            }
            if ((Get-PdbGuid $pdb) -ne $reference.Guid) {
                throw "Shipped PDB does not match its DLL (stale build): $pdb. Rebuild $(Split-Path -Leaf (Split-Path -Parent (Split-Path -Parent $pluginsDir)))."
            }
            $text = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes($pdb))
            $leak = [regex]::Match($text, '[A-Za-z]:[\\/][\x20-\x7e]{2,}')
            if ($leak.Success) {
                throw "Shipped PDB leaks a local path: $pdb -> $($leak.Value)"
            }
        }
    }
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

    Copy-Item -Force -LiteralPath (Join-Path $src "$CompanionModName.esm") -Destination (Join-Path $dst "$CompanionModName.esm")

    # F4SE\ carries the companion plugin DLL and its .pdb, so user crash reports can be
    # symbolicated (the plugin fixes FO4's 16-bit auto-calc health truncation and is a hard
    # runtime dependency). /PDBALTPATH keeps build-machine paths out of the DLL. The .pdb
    # purge above runs on $DistDir before this copy, so it doesn't strip these.
    $runtimeDirs = @("data", "F4SE", "Strings")
    foreach ($dir in $runtimeDirs) {
        $srcDir = Join-Path $src $dir
        if (-not (Test-Path $srcDir)) {
            throw "Companion runtime dir not found: $srcDir"
        }
        $dstDir = Join-Path $dst $dir
        New-Item -ItemType Directory -Force -Path $dstDir | Out-Null
        Get-ChildItem -LiteralPath $srcDir -Recurse -File |
            ForEach-Object {
                $rel = $_.FullName.Substring($srcDir.Length).TrimStart("\")
                if (($dir -eq "F4SE" -and $rel -like "Plugins\B21_TalesFromAppalachia_*.txt") -or
                    ($dir -eq "data" -and $rel -like "Interface\Translations\B21_TalesFromAppalachia_*.txt")) {
                    # BACUP generates the complete table in the converted mod's F4SE tree.
                    # A companion copy could shadow it in MO2 and omit newer converted keys.
                    return
                }
                $out = Join-Path $dstDir $rel
                New-Item -ItemType Directory -Force -Path (Split-Path -Parent $out) | Out-Null
                Copy-Item -Force -LiteralPath $_.FullName -Destination $out
            }
    }

    Write-Host "  Copied: mods/$CompanionModName runtime payload"
}

function Assert-CompanionPluginIni($Root) {
    # Copy-AppalachiaCompanionMod stages F4SE/, so a stale
    # staging folder can ship a missing/outdated INI or leftover per-feature INIs.
    $pluginsDir = Join-Path $Root "mods\$CompanionModName\F4SE\Plugins"
    $stagedIni = Join-Path $pluginsDir "$CompanionModName.ini"
    $sourceIni = Join-Path $RepoRoot "mods\$CompanionModName\$CompanionModName.ini"
    $rebuildHint = "Rebuild Tales first: run 'xmake build' in mods\$CompanionModName."

    if (-not (Test-Path -LiteralPath $stagedIni -PathType Leaf)) {
        throw "Companion plugin INI missing: $stagedIni. $rebuildHint"
    }
    if ((Get-Sha256Hex $stagedIni) -ne (Get-Sha256Hex $sourceIni)) {
        throw "Companion plugin INI is stale: $stagedIni does not match $sourceIni. $rebuildHint"
    }
    $obsolete = Get-ChildItem -LiteralPath $pluginsDir -Filter "${CompanionModName}_*.ini" -File -ErrorAction SilentlyContinue
    if ($obsolete) {
        throw "Companion plugin staging has obsolete INI(s): $($obsolete.Name -join ', '). $rebuildHint"
    }
}

function Assert-BacupTranslationResource($Root) {
    $relative = "bacup_lib\data\translations\B21_TalesFromAppalachia_en.txt"
    $source = Join-Path $BacupRoot "py_bacup_lib\python\$relative"
    $packaged = Join-Path $Root "_internal\$relative"
    if (-not (Test-Path -LiteralPath $packaged -PathType Leaf)) {
        throw "BACUP packaged translation source is missing: $packaged"
    }
    if ((Get-Sha256Hex $source) -ne (Get-Sha256Hex $packaged)) {
        throw "BACUP packaged translation source is stale: $packaged"
    }
}

function Build-AppalachiaCompanionPlugin {
    $modDir = Join-Path $RepoRoot "mods\$CompanionModName"
    Remove-Item Env:\XSE_FO4_MODS_PATH -ErrorAction SilentlyContinue
    Remove-Item Env:\XSE_FO4_GAME_PATH -ErrorAction SilentlyContinue
    Push-Location $modDir
    try {
        & xmake f -m releasedbg -o build\bacup -y
        if ($LASTEXITCODE -ne 0) { throw "xmake configure failed for $CompanionModName ($LASTEXITCODE)" }
        & xmake build -y $CompanionModName
        if ($LASTEXITCODE -ne 0) { throw "xmake build failed for $CompanionModName ($LASTEXITCODE)" }
        # An incremental build can keep a DLL linked before /PDBSTRIPPED was added.
        # Tales stages through after_build; xmake install can restore a stale DLL.
        & xmake build -r --linkonly -y $CompanionModName
        if ($LASTEXITCODE -ne 0) { throw "xmake relink failed for $CompanionModName ($LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
    $dll = Join-Path $modDir "F4SE\Plugins\$CompanionModName.dll"
    $stripped = Join-Path $modDir "build\bacup\$CompanionModName.stripped.pdb"
    if ((Get-PdbGuid $stripped) -ne (Get-DllPdbReference $dll).Guid) {
        throw "Rebuilt $CompanionModName DLL does not match its stripped PDB."
    }
    Write-Host "  Built: mods/$CompanionModName native runtime and stripped symbols"
}

function Build-BundledMod($Mod) {
    $ModName = $Mod.Name
    $target = if ($Mod.Target) { $Mod.Target } else { $ModName }
    $modDir = Join-Path $RepoRoot "mods\$ModName"

    # The commonlibf4 plugin rule installs into these locations instead of the mod
    # directory when they are set, which would write straight into the game's Data.
    Remove-Item Env:\XSE_FO4_MODS_PATH -ErrorAction SilentlyContinue
    Remove-Item Env:\XSE_FO4_GAME_PATH -ErrorAction SilentlyContinue

    foreach ($stagedDir in @("F4SE", "PrismaUI_F4")) {
        Remove-TreeIfPresent (Join-Path $modDir $stagedDir)
    }
    $scriptsDir = Join-Path $modDir "data\Scripts"
    if (Test-Path -LiteralPath $scriptsDir) {
        Get-ChildItem -LiteralPath $scriptsDir -Filter "*.pex" -File | Remove-Item -Force
    }

    # Build from inside the mod directory: running xmake for one mod from another
    # mod's cwd can cross-contaminate CommonLib include directories.
    Push-Location $modDir
    try {
        & xmake f -m releasedbg -y
        if ($LASTEXITCODE -ne 0) { throw "xmake configure failed for $ModName ($LASTEXITCODE)" }
        & xmake build -y $target
        if ($LASTEXITCODE -ne 0) { throw "xmake build failed for $ModName ($LASTEXITCODE)" }
        # The build only stages F4SE/ when it relinks the DLL.
        & xmake install -y $target
        if ($LASTEXITCODE -ne 0) { throw "xmake install failed for $ModName ($LASTEXITCODE)" }
    } finally {
        Pop-Location
    }

    if ($Mod.NativeOnly) {
        Write-Host "  Built: mods/$ModName"
        return
    }

    Push-Location $RepoRoot
    try {
        & modkit.exe mod compile $ModName --verify-stock
        if ($LASTEXITCODE -ne 0) { throw "Papyrus compile failed for $ModName ($LASTEXITCODE)" }
        if (Test-Path (Join-Path $modDir "yaml\plugin.yaml")) {
            & modkit.exe esp build-mod $ModName
            if ($LASTEXITCODE -ne 0) { throw "Plugin build failed for $ModName ($LASTEXITCODE)" }
            # v1 archives load on both pre-next-gen and next-gen runtimes; v8 only on next-gen.
            & modkit.exe build pack $ModName --pc --ba2-target og
            if ($LASTEXITCODE -ne 0) { throw "BA2 pack failed for $ModName ($LASTEXITCODE)" }
        }
    } finally {
        Pop-Location
    }

    Write-Host "  Built: mods/$ModName"
}

function Copy-BundledMod($DestinationRoot, $Mod) {
    $modName = $Mod.Name
    $src = Join-Path $RepoRoot "mods\$modName"
    $dst = Join-Path $DestinationRoot "mods\$modName"
    Remove-TreeIfPresent $dst
    New-Item -ItemType Directory -Force -Path $dst | Out-Null

    $runtimeFiles = @($Mod.RootFiles)
    if ($Mod.RuntimeFiles) { $runtimeFiles += $Mod.RuntimeFiles }
    foreach ($file in $runtimeFiles) {
        $srcFile = Join-Path $src $file
        if (-not (Test-Path -LiteralPath $srcFile -PathType Leaf)) {
            throw "Bundled mod file not found: $srcFile"
        }
        if ((Get-Item -LiteralPath $srcFile).Length -eq 0) {
            throw "Bundled mod file is empty: $srcFile"
        }
        $dstFile = Join-Path $dst $file
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dstFile) | Out-Null
        Copy-Item -Force -LiteralPath $srcFile -Destination $dstFile
    }
    foreach ($dir in $Mod.RuntimeDirs) {
        $srcDir = Join-Path $src $dir
        if (-not (Test-Path -LiteralPath $srcDir)) {
            throw "Bundled mod runtime dir not found: $srcDir"
        }
        $dstDir = Join-Path $dst $dir
        New-Item -ItemType Directory -Force -Path $dstDir | Out-Null
        Copy-Item -Recurse -Force -Path "$srcDir\*" -Destination $dstDir
    }

    # Same rule as B21_FullScreenMap's make-release.ps1: custom map folders ship as
    # README templates only. An image there overrides that worldspace's default map.
    $customMapsDir = Join-Path $dst "F4SE\Plugins\$modName\maps\custom"
    if (Test-Path -LiteralPath $customMapsDir) {
        Get-ChildItem -LiteralPath $customMapsDir -Recurse -File |
            Where-Object { $_.Extension.ToLowerInvariant() -in @(".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".dds") } |
            Remove-Item -Force
    }
    # Conversion writes the Appalachia map pack; Color mode falls back to it.
    $appalachiaColorMap = Join-Path $dst "F4SE\Plugins\$modName\maps\colorized\appalachia.dds"
    if (Test-Path -LiteralPath $appalachiaColorMap) {
        Remove-Item -Force -LiteralPath $appalachiaColorMap
    }

    if ($Mod.IniOverrides) {
        foreach ($iniRel in $Mod.IniOverrides.Keys) {
            $iniPath = Join-Path $dst $iniRel
            $text = [IO.File]::ReadAllText($iniPath)
            foreach ($entry in $Mod.IniOverrides[$iniRel].GetEnumerator()) {
                $pattern = "(?m)^$([regex]::Escape($entry.Key))=[^\r\n]*"
                # A renamed key would otherwise ship its dev value silently.
                if (-not [regex]::IsMatch($text, $pattern)) {
                    throw "Release ini override key not found: $($entry.Key) in $iniPath"
                }
                $text = [regex]::Replace($text, $pattern, "$($entry.Key)=$($entry.Value)")
            }
            [IO.File]::WriteAllText($iniPath, $text)
        }
    }

    Write-Host "  Copied: mods/$modName runtime payload"
}

Write-Host "=== B.A.C.U.P. - Bethesda Asset Converter Universal Platform (v$Version, $(if ($OneFile) {'single-file'} else {'folder'})) ===" -ForegroundColor Cyan
Warn-IgnoredSteamworksSourceFiles

# Step 1: Clean only this variant's previous output
Write-Host "`n[1/6] Cleaning previous build..." -ForegroundColor Yellow
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
foreach ($bundledMod in $BundledMods) {
    Remove-TreeIfPresent (Join-Path $RepoRoot "dist\mods\$($bundledMod.Name)")
}
# Drop payloads left behind by earlier builds that still bundled these mods.
foreach ($retiredModName in $RetiredModNames) {
    Remove-TreeIfPresent (Join-Path $RepoRoot "dist\mods\$retiredModName")
}

# Step 2: Rebuild the bundled mods from source so the payload never ships a stale
# DLL, script, plugin or archive.
Write-Host "`n[2/6] Building bundled mods..." -ForegroundColor Yellow
if (Test-Path (Join-Path $RepoRoot "mods\$CompanionModName")) {
    Build-AppalachiaCompanionPlugin
}
foreach ($bundledMod in $BundledMods) {
    if (Test-Path (Join-Path $RepoRoot "mods\$($bundledMod.Name)")) {
        Build-BundledMod $bundledMod
    } else {
        Write-Host "  skip $($bundledMod.Name) (not present)"
    }
}

# Scaleform conversion lives in creation_lib; both extensions must be fresh.
Write-Host "`n[3/6] Ensuring native extensions are current..." -ForegroundColor Yellow
$EnsureNativeScript = Join-Path $RepoRoot "scripts\ensure_native.py"
& uv run python $EnsureNativeScript --package all
if ($LASTEXITCODE -ne 0) {
    Write-Error "BACUP native ensure failed with exit code $LASTEXITCODE"
    exit 1
}

$InstalledNative = Join-Path $BacupRoot "py_bacup_lib\python\bacup_lib\_native.pyd"
$BuiltNative = Join-Path $BacupRoot "py_bacup_lib\target\maturin\_native.dll"
if (-not (Test-Path $InstalledNative) -or -not (Test-Path $BuiltNative)) {
    throw "BACUP native ensure did not find both installed and staged native binaries."
}
if ((Get-Sha256Hex $InstalledNative) -ne (Get-Sha256Hex $BuiltNative)) {
    throw "Installed bacup_lib._native.pyd does not match the freshly built native DLL."
}

# Step 4: Run PyInstaller. Exe-name/icon/onefile are selected via env vars (the
# spec reads them); MODBOX21_ONEFILE folds binaries + resource into one EXE.
Write-Host "`n[4/6] Running PyInstaller..." -ForegroundColor Yellow
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

# Step 5: onedir builds need resource/ copied beside the EXE; onefile bundles
# resource/ in the archive and needs nothing copied.
if (-not $OneFile) {
    Write-Host "`n[5/6] Copying runtime payload (onedir)..." -ForegroundColor Yellow
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
    Write-Host "`n[5/6] Single-file build: resource bundled in the EXE, nothing to copy." -ForegroundColor Yellow
}

$PayloadRoot = if ($OneFile) { Join-Path $RepoRoot "dist" } else { Join-Path $RepoRoot "dist\$Folder" }
if (Test-Path (Join-Path $RepoRoot "mods\$CompanionModName")) {
    Copy-AppalachiaCompanionMod $PayloadRoot
    Assert-CompanionPluginIni $PayloadRoot
} else {
    Write-Host "  skip companion mod (not present)"
}
foreach ($bundledMod in $BundledMods) {
    if (Test-Path (Join-Path $RepoRoot "mods\$($bundledMod.Name)")) {
        Copy-BundledMod $PayloadRoot $bundledMod
    } else {
        Write-Host "  skip $($bundledMod.Name) (not present)"
    }
}
Remove-SteamworksPayloadFiles $PayloadRoot
Assert-NoBundledGameAssets $PayloadRoot
Assert-NoSteamworksPayloadFiles $PayloadRoot
Install-CleanPluginPdbs $PayloadRoot
Assert-CleanPluginPdbs $PayloadRoot
if (-not $OneFile) {
    Assert-NoDeveloperPayload $PayloadRoot
    Assert-BacupTranslationResource $PayloadRoot
}

# Step 6: Report output location
Write-Host "`n[6/6] Done." -ForegroundColor Yellow
if ($OneFile) {
    $exePath = Join-Path $RepoRoot "dist\$ExeName.exe"
} else {
    $exePath = Join-Path $RepoRoot "dist\$Folder\$ExeName.exe"
}
$size = if (Test-Path $exePath) { "{0:N0} MB" -f ((Get-Item $exePath).Length / 1MB) } else { "?" }
Write-Host "`nEXE: $exePath  ($size)" -ForegroundColor Green
