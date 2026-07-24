from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _extend_script_names_with_ancestor_closure,
    _merge_script_method_patches,
    _script_key,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
# The two record-unbound parents this task's closure fix targets (see
# .superpowers/sdd/task-parent-closure-brief.md). Neither is ever VMAD-bound
# directly -- only descendants like V96_1_CryoPipeScript / VaultCircuitBreakerScript
# are -- so their only available source is the raw FO76 client extraction.
FO76_EXTRACTED_SCRIPTS = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"

pytestmark = pytest.mark.skipif(
    not FO76_EXTRACTED_SCRIPTS.is_dir(),
    reason="FO76 extracted client scripts unavailable",
)


class _RecordingRunner:
    def __init__(self) -> None:
        self.logs: list[tuple[str, str]] = []

    def emit_log(self, level: str, message: str) -> None:
        self.logs.append((level, message))


def _real_source_index() -> dict[str, Path]:
    return {path.stem.lower(): path for path in FO76_EXTRACTED_SCRIPTS.glob("*.pex")}


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _merged_parent_source(pex_stem: str) -> tuple[str, str]:
    """Decompile + patch-merge a closure-emitted parent, exactly like
    `_decompile_script_source_for_fo4` does for any record-bound script."""
    skeleton = decompile_pex(
        str(FO76_EXTRACTED_SCRIPTS / f"{pex_stem}.pex"),
        type_adapter=None,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    script_name = skeleton.splitlines()[0].split()[1]
    patch = _script_patch_source(script_name)
    assert patch is not None, f"expected a durable patch for {script_name}"
    return script_name, _merge_script_method_patches(skeleton, patch)


# --- closure helper -----------------------------------------------------------


def test_closure_adds_missing_parent_chain_from_two_real_vault_children():
    source_index = _real_source_index()
    # FO76 ships its own compiled stubs for engine base classes alongside real
    # mod scripts, so ObjectReference *is* present in source_index too -- only
    # target_index (mirroring a real FO4 install scan) is what should stop the
    # walk there, exactly like production's target-vs-source split.
    target_index: dict[str, Path | None] = {_script_key("ObjectReference"): None}
    script_names_by_key = {
        _script_key("V96_1_CryoPipeScript"): "V96_1_CryoPipeScript",
        _script_key("VaultCircuitBreakerScript"): "VaultCircuitBreakerScript",
    }

    _extend_script_names_with_ancestor_closure(
        script_names_by_key,
        source_index=source_index,
        target_index=target_index,
        runner=_RecordingRunner(),
    )

    assert _script_key("VaultDefaultMultiStateActivator") in script_names_by_key
    assert _script_key("VaultDefault2StateActivator") in script_names_by_key
    # ObjectReference resolves in the FO4 target -- the closure must stop there,
    # not shadow it, not loop.
    assert _script_key("ObjectReference") not in script_names_by_key


def test_closure_skips_fo4_native_ancestor():
    source_index = _real_source_index()
    # Simulate ObjectReference already resolving in the FO4 target -- the closure
    # must never add or shadow it, even though it has no FO76 client PEX either.
    target_index: dict[str, Path | None] = {_script_key("ObjectReference"): None}
    script_names_by_key = {
        _script_key(
            "VaultDefaultMultiStateActivator"
        ): "VaultDefaultMultiStateActivator",
    }

    _extend_script_names_with_ancestor_closure(
        script_names_by_key,
        source_index=source_index,
        target_index=target_index,
        runner=_RecordingRunner(),
    )

    assert _script_key("ObjectReference") not in script_names_by_key
    assert len(script_names_by_key) == 1


def test_closure_adds_each_parent_exactly_once():
    source_index = _real_source_index()
    target_index: dict[str, Path | None] = {}
    # Seed with both a child AND its parent already present -- the parent must
    # not be duplicated or re-queued once it's already in the emit set.
    script_names_by_key = {
        _script_key("V96_1_CryoPipeScript"): "V96_1_CryoPipeScript",
        _script_key(
            "VaultDefaultMultiStateActivator"
        ): "VaultDefaultMultiStateActivator",
    }

    _extend_script_names_with_ancestor_closure(
        script_names_by_key,
        source_index=source_index,
        target_index=target_index,
        runner=_RecordingRunner(),
    )

    names = list(script_names_by_key.values())
    assert names.count("VaultDefaultMultiStateActivator") == 1


def test_closure_warns_once_and_leaves_unemitted_when_no_pex_anywhere():
    # Two fake children both "extend" a name absent from both indexes (reuse a
    # real PEX file for the parse -- only its declared `.parent` matters here).
    real_child_pex = FO76_EXTRACTED_SCRIPTS / "v96_1_cryopipescript.pex"
    source_index: dict[str, Path] = {
        _script_key("FakeChildOne"): real_child_pex,
        _script_key("FakeChildTwo"): real_child_pex,
    }
    target_index: dict[str, Path | None] = {}
    script_names_by_key = {
        _script_key("FakeChildOne"): "FakeChildOne",
        _script_key("FakeChildTwo"): "FakeChildTwo",
    }
    runner = _RecordingRunner()

    _extend_script_names_with_ancestor_closure(
        script_names_by_key,
        source_index=source_index,
        target_index=target_index,
        runner=runner,
    )

    # v96_1_cryopipescript.pex declares Extends VaultDefaultMultiStateActivator,
    # which has no PEX under either fixture index here.
    assert _script_key("VaultDefaultMultiStateActivator") not in script_names_by_key
    assert len(script_names_by_key) == 2
    warn_count = sum(1 for level, _msg in runner.logs if level == "WARN")
    assert warn_count == 1


# --- compile verification -----------------------------------------------------


@pytest.mark.parametrize(
    "pex_stem",
    ["vaultdefaultmultistateactivator", "vaultdefault2stateactivator"],
)
def test_vault_parent_decompiled_and_patched_source_compiles(pex_stem: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    script_name, merged = _merged_parent_source(pex_stem)
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_vault_children_compile_against_closure_emitted_parents(tmp_path):
    # Proves the actual end-to-end fix: once Part 1 emits the parents as real
    # decompiled+patched source (not the raw FO76 PEX fallback the vault-misc
    # shard's own test uses), the record-bound children still compile against
    # them via ordinary source-directory resolution.
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    parent_dir = tmp_path / "parents"
    parent_dir.mkdir()
    for pex_stem in ("vaultdefaultmultistateactivator", "vaultdefault2stateactivator"):
        script_name, merged = _merged_parent_source(pex_stem)
        (parent_dir / f"{script_name}.psc").write_text(merged, encoding="utf-8")

    for child in ("V96_1_CryoPipeScript", "VaultCircuitBreakerScript"):
        source_path = SOURCE_ROOT / _script_relative_path(child, ".psc")
        if not source_path.is_file():
            pytest.skip(f"{child} not yet decompiled to Source/User")
        skeleton = source_path.read_text(encoding="utf-8")
        # The vault-misc shard's own fragments for these two children land in
        # parallel with this task; if not committed yet, fall back to compiling
        # the hollow skeleton against the parents instead (brief-sanctioned).
        patch = _script_patch_source(child)
        merged = _merge_script_method_patches(skeleton, patch) if patch else skeleton

        result = compile_psc(
            merged,
            imports=[str(base_source), str(parent_dir)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(child, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{child}: {diagnostics}"
