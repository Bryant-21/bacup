<#
Fabricates the output of `WwiseConsole generate-soundbank <proj> --bank <name>
--platform Windows --language SFX --soundbank-path Windows <outDir>`:
reads the Events/FO4SF.wwu + Actor-Mixer Hierarchy/FO4SF.wwu written by
build_bank, and writes <outDir>/<name>.bnk + <name>.json (+ the aggregate
SoundbanksInfo.json CopyStreamedFiles reads) with one IncludedEvents /
ReferencedStreamedFiles entry per event, echoing back the same object GUIDs
and MediaIDs build_bank authored. Also increments a call-count file so the
test can assert exactly one invocation for the whole batch.
#>
$ErrorActionPreference = "Stop"
$argv = $args

function Get-ArgAfter($flag) {
    for ($i = 0; $i -lt $argv.Length; $i++) {
        if ($argv[$i] -eq $flag) { return $argv[$i + 1] }
    }
    return $null
}

function JEsc([string]$s) {
    if ($null -eq $s) { return "" }
    return $s -replace '\\', '\\\\' -replace '"', '\"'
}

$proj = $argv[1]
$bankName = Get-ArgAfter "--bank"
$outDir = $argv[$argv.Length - 1]

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$counterFile = Join-Path $outDir "_wwiseconsole_call_count.txt"
$count = 0
if (Test-Path $counterFile) { $count = [int](Get-Content $counterFile -Raw) }
$count++
Set-Content -Path $counterFile -Value $count -NoNewline

[xml]$eventsXml = Get-Content (Join-Path $proj "Events\FO4SF.wwu") -Raw
[xml]$amhXml = Get-Content (Join-Path $proj "Actor-Mixer Hierarchy\FO4SF.wwu") -Raw

$mediaMap = @{}
foreach ($sound in $amhXml.SelectNodes("//Sound")) {
    $name = $sound.GetAttribute("Name")
    $mediaNode = $sound.SelectSingleNode(".//MediaID")
    if ($mediaNode) { $mediaMap[$name] = $mediaNode.GetAttribute("ID") }
}

$includedEventsParts = @()
$referencedFilesParts = @()
foreach ($event in $eventsXml.SelectNodes("//Event")) {
    $ename = $event.GetAttribute("Name")
    $eguid = $event.GetAttribute("ID")
    $mediaId = $mediaMap[$ename]
    $includedEventsParts += ('{{"Id":"0","Name":"{0}","GUID":"{1}","DurationType":"OneShot"}}' -f (JEsc $ename), (JEsc $eguid))
    $referencedFilesParts += ('{{"Id":"{0}","Language":"SFX","ShortName":"{1}","Path":"{0}.wem"}}' -f (JEsc $mediaId), (JEsc $ename))
}

$includedEventsJson = $includedEventsParts -join ","
$referencedFilesJson = $referencedFilesParts -join ","

$bankJson = '{{"SoundBanksInfo":{{"Platform":"Windows","SchemaVersion":"12","SoundbankVersion":"140","SoundBanks":[{{"Id":"0","GUID":"{{00000000-0000-0000-0000-000000000000}}","ShortName":"{0}","Path":"{0}.bnk","IncludedEvents":[{1}],"ReferencedStreamedFiles":[{2}]}}]}}}}' -f (JEsc $bankName), $includedEventsJson, $referencedFilesJson

Set-Content -Path (Join-Path $outDir "$bankName.json") -Value $bankJson -NoNewline
Set-Content -Path (Join-Path $outDir "SoundbanksInfo.json") -Value $bankJson -NoNewline
Set-Content -Path (Join-Path $outDir "$bankName.bnk") -Value "FAKEBNK:$bankName" -NoNewline
