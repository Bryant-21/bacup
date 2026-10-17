# FOMOD reference fixture evidence

Additional local context, source binding/retry, required-conflict controls and bounded
title/header rendering now have generated service/input coverage in
[INSTALLER_COVERAGE.md](INSTALLER_COVERAGE.md). The source-derived corpus below remains
independent of execution of the official Qt engine.

The reproducible corpus contains **70 original generated configurations** spanning schema
versions 1–5. All 70 expected validity outcomes match the already installed offline
`.NET System.Xml.Schema` validator: 60 valid configurations and ten invalid enum/integer
configurations. Both manager policies also match the explicit source-derived plan expectations.
This is **not execution of MO2's Qt installer engine** or a real-mod certification.

Corpus: [fomod_reference_corpus.py](tests/fomod_reference_corpus.py). Semantic regression:
[test_fomod_reference_corpus.py](tests/test_fomod_reference_corpus.py). Independent offline
schema oracle: [validate_fomod_schemas.ps1](tests/validate_fomod_schemas.ps1).

The corpus covers required files, signed priority, document-order collisions, all five group
cardinalities/defaults, MO2 forced unavailable options, last-recommended radio defaults,
duplicate/trimmed flags, invalid enums/integers and explicit differences between schema
validity and safe file planning. Windows traversal, missing payloads and zero-file installation
are explicit manager errors even when the schema accepts the XML. Existing parser/compatibility
cases separately cover dependencies, versions, previous-page flag scope, visibility, unknown
file state, encoding/DTD guards, folders, alwaysInstall/installIfUsable and transaction context.

Reference expectations use MO2's independently reviewed
[installer presentation/conditions](https://github.com/ModOrganizer2/modorganizer-installer_fomod/blob/24f07cf7af5273efe52daa90c631a49e7e847c74/src/fomodinstallerdialog.cpp)
at revision `24f07cf7af5273efe52daa90c631a49e7e847c74`.
The schemas are pinned to Nexus Mod Manager revision
`b6d77fc52cf9dc82c909399313ba562c938a8af2`:
[versions 1–4](https://github.com/Nexus-Mods/Nexus-Mod-Manager/tree/b6d77fc52cf9dc82c909399313ba562c938a8af2/Game%20Modes/Fallout3.XmlScript/data)
and the [common 5.0 schema](https://github.com/Nexus-Mods/Nexus-Mod-Manager/blob/b6d77fc52cf9dc82c909399313ba562c938a8af2/Script%20Types/XmlScript/Schemas/XmlScript5.0.xsd).
The earlier schemas live in the upstream FO3 package; these fixtures exercise common
declarative grammar, not a new FO3 runtime adapter or all game-specific extensions.

Official schema files remain outside the repository. Their URLs, revision and SHA-256 hashes
are in the temporary `mod-manager-fomod-differential/schemas/provenance.json`. Inputs, both
policy results and oracle results are retained in its `corpus/corpus.json` and
`corpus/schema-result.json`. No upstream GPL engine/source/artwork was incorporated in the
manager. No real package, source mod or game file was used.

Reproduce against an explicitly prepared local folder of the pinned schemas:

```powershell
uv run --no-sync python -m ui.mod_manager.tests.fomod_reference_corpus --output <temporary-generated-corpus>
./ui/mod_manager/tests/validate_fomod_schemas.ps1 -Corpus <temporary-generated-corpus>/corpus.json -Schemas <pinned-local-schema-folder>
uv run --no-sync pytest ui/mod_manager/tests/test_fomod_reference_corpus.py -q
```

The oracle prohibits DTDs/network resolution and ignores document schemaLocation instructions.
It uses only the selected local schema and bounded generated input. Qt/PySide/PyQt/MO2 host
interfaces are unavailable in the current execution host; installed CMake/Clang and .NET alone
do not provide an executable official FOMOD plugin host. An approved disposable MO2/Qt reference
host and agreed broader corpus are needed for actual-engine differential evidence. There is no
pending approval for this independent local fixture work. Official Qt sequencing, additional
codec heuristics, engine archive attachment and executable C#/Python installers remain outside
certification. Local repair/context/presentation coverage is defined in the continuation.
