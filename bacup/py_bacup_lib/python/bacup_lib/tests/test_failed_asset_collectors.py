from bacup_lib.workflows.unified import _UnifiedRecordRuntime


def test_failed_paths_collects_warn_not_found():
    lines = [
        "[WARN] NIF not found: Meshes/a.nif: source path did not resolve",
        "[ERROR] NIF failed: Meshes/b.nif -> boom",
        "[INFO] NIF: unrelated",
    ]
    result = _UnifiedRecordRuntime._failed_paths(lines, "NIF")
    assert "Meshes/a.nif: source path did not resolve" in result
    assert any(item.startswith("Meshes/b.nif") for item in result)


def test_failed_paths_still_collects_legacy_error_not_found():
    lines = ["[ERROR] Texture not found: Textures/x.dds: missing"]
    result = _UnifiedRecordRuntime._failed_paths(lines, "Texture")
    assert "Textures/x.dds: missing" in result


def test_failed_material_paths_collects_warn_not_found():
    lines = ["[WARN] Material not found: Materials/x.bgsm: missing"]
    result = _UnifiedRecordRuntime._failed_material_paths(lines)
    assert "Materials/x.bgsm: missing" in result
