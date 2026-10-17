param([Parameter(Mandatory = $true)][string]$Corpus, [Parameter(Mandatory = $true)][string]$Schemas)
$ErrorActionPreference = 'Stop'
# Original offline oracle. Never honor XML schemaLocation or resolve network URLs.
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Xml;
using System.Xml.Schema;
public static class ModBoxFomodSchemaOracle {
    public static string[] Validate(string schemaFile, string xmlFile) {
        var errors = new List<string>();
        var schemas = new XmlSchemaSet();
        schemas.XmlResolver = null;
        var schemaSettings = new XmlReaderSettings { DtdProcessing = DtdProcessing.Prohibit, XmlResolver = null, MaxCharactersInDocument = 2000000 };
        using (var schemaReader = XmlReader.Create(schemaFile, schemaSettings)) { schemas.Add(null, schemaReader); }
        schemas.Compile();
        var settings = new XmlReaderSettings { DtdProcessing = DtdProcessing.Prohibit, XmlResolver = null, ValidationType = ValidationType.Schema, Schemas = schemas, MaxCharactersInDocument = 2000000 };
        settings.ValidationFlags = XmlSchemaValidationFlags.ReportValidationWarnings;
        settings.ValidationEventHandler += (sender, e) => errors.Add(e.Message);
        using (var reader = XmlReader.Create(xmlFile, settings)) { while (reader.Read()) {} }
        return errors.ToArray();
    }
}
'@
$manifest = Get-Content -LiteralPath $Corpus -Raw -Encoding UTF8 | ConvertFrom-Json
$results = foreach ($case in $manifest.cases) {
    $schema = Join-Path $Schemas ('XmlScript' + $case.version + '.0.xsd')
    $errors = @([ModBoxFomodSchemaOracle]::Validate($schema, $case.path))
    [pscustomobject]@{ name = $case.name; valid = ($errors.Count -eq 0); expected = $case.xsd_expected; errors = $errors }
}
$report = [pscustomobject]@{ validator = '.NET System.Xml.Schema offline'; official_engine_executed = $false; cases = @($results); mismatches = @($results | Where-Object { $_.valid -ne $_.expected }) }
$output = Join-Path (Split-Path -Parent $Corpus) 'schema-result.json'
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $output -Encoding UTF8
Write-Output ('Validated {0} generated configurations; {1} schema mismatches; {2}' -f $results.Count, $report.mismatches.Count, $output)
if ($report.mismatches.Count) { exit 1 }
