<#
Fabricates the output of `CopyStreamedFiles -info <json> -outputpath <dir>
-banks <banklist> -languages SFX`: reads the aggregate SoundbanksInfo.json
written by wwise_console_stub.ps1 and writes one flat <mediaId>.wem per
ReferencedStreamedFiles entry.
#>
$ErrorActionPreference = "Stop"
$argv = $args

function Get-ArgAfter($flag) {
    for ($i = 0; $i -lt $argv.Length; $i++) {
        if ($argv[$i] -eq $flag) { return $argv[$i + 1] }
    }
    return $null
}

$infoPath = Get-ArgAfter "-info"
$outDir = Get-ArgAfter "-outputpath"

$json = Get-Content $infoPath -Raw | ConvertFrom-Json
foreach ($bank in @($json.SoundBanksInfo.SoundBanks)) {
    foreach ($f in @($bank.ReferencedStreamedFiles)) {
        $wemPath = Join-Path $outDir "$($f.Id).wem"
        Set-Content -Path $wemPath -Value "FAKEWEM:$($f.Id)" -NoNewline
    }
}
