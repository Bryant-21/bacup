param()

$ErrorActionPreference = "Stop"

$BacupRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $BacupRoot
$SpecFile = Join-Path $BacupRoot "BACUP-Regen.spec"
$WorkPath = Join-Path $RepoRoot "build\bacup-regen"
$StagingPath = Join-Path $RepoRoot "build\bacup-regen-staging"
$WheelPath = Join-Path $StagingPath "wheels"
$PackagePath = Join-Path $StagingPath "packages"
$DistPath = Join-Path $RepoRoot "dist\BACUP-Regen"
$ExePath = Join-Path $DistPath "BACUP-Regen.exe"
$PythonExe = Join-Path $RepoRoot ".venv\Scripts\python.exe"
$SourceEnvPath = Join-Path $RepoRoot ".env"
$FrozenEnvPath = Join-Path $DistPath ".env"
$ConversionEnvNames = @(
    "FO4_DIR", "FO4_DATA_DIR", "FO4_EXTRACTED_DIR",
    "FO76_DIR", "FO76_DATA_DIR", "FO76_EXTRACTED_DIR",
    "FONV_DIR", "FONV_DATA_DIR", "FONV_EXTRACTED_DIR",
    "FO3_DIR", "FO3_DATA_DIR", "FO3_EXTRACTED_DIR",
    "SKYRIMSE_DIR", "SKYRIMSE_DATA_DIR", "SKYRIMSE_EXTRACTED_DIR"
)

function Remove-TreeIfPresent($Path) {
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -Recurse -Force -LiteralPath $Path
    }
}

function Build-NativeWheel($ProjectPath, $WheelPrefix) {
    Push-Location $ProjectPath
    try {
        & $PythonExe -m maturin build --release --out $WheelPath
        if ($LASTEXITCODE -ne 0) {
            throw "Native wheel build failed for $ProjectPath with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    $wheels = @(Get-ChildItem -LiteralPath $WheelPath -Filter "$WheelPrefix-*.whl")
    if ($wheels.Count -ne 1) {
        throw "Expected one $WheelPrefix wheel, found $($wheels.Count)."
    }

    & $PythonExe -m zipfile -e $wheels[0].FullName $PackagePath
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to extract native wheel $($wheels[0].FullName)."
    }
}

function Write-ConversionEnvSnapshot($Source, $Destination) {
    if (-not (Test-Path -LiteralPath $Source)) {
        throw "Conversion environment source not found: $Source"
    }

    $snapshotLines = Get-Content -LiteralPath $Source | Where-Object {
        if ($_ -notmatch '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=') {
            return $false
        }
        return $ConversionEnvNames -contains $Matches[1]
    }
    if (-not $snapshotLines) {
        throw "Conversion environment snapshot would be empty."
    }

    [System.IO.File]::WriteAllLines(
        $Destination,
        $snapshotLines,
        (New-Object System.Text.UTF8Encoding($false))
    )
    $unexpectedNames = Get-Content -LiteralPath $Destination | ForEach-Object {
        if ($_ -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=') {
            $Matches[1]
        }
    } | Where-Object { $ConversionEnvNames -notcontains $_ }
    if ($unexpectedNames) {
        throw "Conversion environment snapshot contains unexpected keys: $($unexpectedNames -join ', ')"
    }
}

Write-Host "=== BACUP frozen regen runner (onedir) ===" -ForegroundColor Cyan

Write-Host "`n[1/4] Cleaning previous runner build..." -ForegroundColor Yellow
Remove-TreeIfPresent $DistPath
Remove-TreeIfPresent $WorkPath
Remove-TreeIfPresent $StagingPath
New-Item -ItemType Directory -Path $WheelPath, $PackagePath | Out-Null

Write-Host "`n[2/4] Building isolated native snapshots..." -ForegroundColor Yellow
Build-NativeWheel (Join-Path $RepoRoot "py_creation_lib") "py_creation_lib"
Build-NativeWheel (Join-Path $BacupRoot "py_bacup_lib") "bacup_lib"
foreach ($nativePath in @(
    (Join-Path $PackagePath "creation_lib\_native.pyd"),
    (Join-Path $PackagePath "bacup_lib\_native.pyd")
)) {
    if (-not (Test-Path -LiteralPath $nativePath)) {
        throw "Native wheel did not contain expected extension: $nativePath"
    }
}

Write-Host "`n[3/4] Building BACUP-Regen.exe..." -ForegroundColor Yellow
Push-Location $RepoRoot
try {
    $env:BACUP_REGEN_PACKAGE_ROOT = $PackagePath
    & uv run --no-sync --with pyinstaller pyinstaller $SpecFile --noconfirm --workpath $WorkPath
    if ($LASTEXITCODE -ne 0) {
        throw "PyInstaller failed with exit code $LASTEXITCODE"
    }
} finally {
    Remove-Item Env:BACUP_REGEN_PACKAGE_ROOT -ErrorAction SilentlyContinue
    Pop-Location
}
Write-ConversionEnvSnapshot $SourceEnvPath $FrozenEnvPath

Write-Host "`n[4/4] Smoke-testing frozen CLI..." -ForegroundColor Yellow
$nativeOutput = & $ExePath --check-native
if ($LASTEXITCODE -ne 0) {
    throw "Frozen native load smoke failed."
}
if (($nativeOutput -join "`n") -notmatch [regex]::Escape((Join-Path $DistPath "_internal"))) {
    throw "Frozen native smoke did not load extensions from the onedir snapshot."
}
foreach ($arguments in @(
    @("--help"),
    @("--pair", "fnvfo3:fo4", "--help"),
    @("--pair", "skyrimse:fo4", "--help")
)) {
    & $ExePath @arguments | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Frozen CLI smoke failed: $($arguments -join ' ')"
    }
}

Write-Host "`nEXE: $ExePath" -ForegroundColor Green
Write-Host "Conversion environment snapshot: $FrozenEnvPath"
