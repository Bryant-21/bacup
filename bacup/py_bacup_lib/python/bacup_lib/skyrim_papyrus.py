from __future__ import annotations

import hashlib
import re
import tempfile
from dataclasses import dataclass, fields, is_dataclass
from pathlib import Path
from typing import Any, Sequence


@dataclass(frozen=True)
class SkyrimScriptIntent:
    class_name: str
    kind: str
    required: bool = True


@dataclass(frozen=True)
class SkyrimPscSourceArtifact:
    class_name: str
    source: str
    kind: str
    required: bool

    def to_native_payload(self) -> dict[str, object]:
        return {
            "class_name": self.class_name,
            "source": self.source,
            "kind": self.kind,
            "required": self.required,
        }


@dataclass(frozen=True)
class SkyrimScriptReceipt:
    class_name: str
    origin: str
    container: str
    member: str | None
    input_sha256: str
    source_sha256: str
    adapters: tuple[str, ...]
    fo4_validation: str


@dataclass(frozen=True)
class SkyrimPapyrusProductionResult:
    supported: bool
    artifacts: tuple[SkyrimPscSourceArtifact, ...]
    receipts: tuple[SkyrimScriptReceipt, ...]
    unsupported_reason: str | None


@dataclass(frozen=True)
class _SourceCandidate:
    origin: str
    container: Path
    member: str | None = None
    path: Path | None = None


_SKYRIM_TO_FO4_TYPE_ALIASES = {
    "actorvalueinfo": "ActorValue",
    "apparatus": "Form",
    "armoraddon": "Form",
    "art": "Form",
    "colorform": "Form",
    "sounddescriptor": "Sound",
    "treeobject": "Form",
}
_TYPE_FIELDS = frozenset({"parent", "type", "return_type", "target_type", "element_type"})
_CLASS_NAME_CHARS = frozenset(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_:"
)

_FUNCTION_HEADER_RE = re.compile(
    r"^\s*function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)\s*$",
    re.IGNORECASE,
)
_MINIMAL_QUEST_CALL_RE = re.compile(
    r"^\s*(?:self\s*\.\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\((.*?)\)\s*$",
    re.IGNORECASE,
)


def skyrim_type_for_fo4(type_name: str) -> str:
    base = str(type_name or "")
    suffix = ""
    while base.endswith("[]"):
        base = base[:-2]
        suffix += "[]"
    return f"{_SKYRIM_TO_FO4_TYPE_ALIASES.get(base.lower(), base)}{suffix}"


def extract_minimal_quest_fragment_actions(
    source: str,
    function_name: str,
) -> list[dict[str, object]]:
    target = function_name.strip().casefold()
    if not target:
        raise ValueError("minimal quest fragment function name is empty")
    bodies: list[list[str]] = []
    active: list[str] | None = None
    for raw_line in source.splitlines():
        line = raw_line.split(";", 1)[0].strip()
        if active is None:
            header = _FUNCTION_HEADER_RE.match(line)
            if header is not None and header.group(1).casefold() == target:
                active = []
            continue
        if line.casefold() == "endfunction":
            bodies.append(active)
            active = None
            continue
        active.append(line)
    if active is not None:
        raise ValueError(f"minimal quest fragment {function_name} is unterminated")
    if len(bodies) != 1:
        raise ValueError(
            f"minimal quest fragment {function_name} must occur exactly once"
        )

    actions: list[dict[str, object]] = []
    for line in bodies[0]:
        if not line or line.casefold() == "return":
            continue
        call = _MINIMAL_QUEST_CALL_RE.match(line)
        if call is None:
            raise ValueError(
                f"minimal quest fragment {function_name} has unsupported statement: {line}"
            )
        name = call.group(1).casefold()
        args = [item.strip() for item in call.group(2).split(",") if item.strip()]
        if name in {"setobjectivedisplayed", "setobjectivecompleted"}:
            if len(args) not in {1, 2}:
                raise ValueError(f"{call.group(1)} requires an index and optional bool")
            index = _minimal_quest_u16(args[0], call.group(1))
            value = True if len(args) == 1 else _minimal_quest_bool(args[1], call.group(1))
            actions.append(
                {
                    "kind": (
                        "set_objective_displayed"
                        if name == "setobjectivedisplayed"
                        else "set_objective_completed"
                    ),
                    "index": index,
                    "displayed" if name == "setobjectivedisplayed" else "completed": value,
                }
            )
        elif name == "setstage":
            if len(args) != 1:
                raise ValueError("SetStage requires exactly one index")
            actions.append(
                {"kind": "set_stage", "index": _minimal_quest_u16(args[0], "SetStage")}
            )
        elif name in {"completequest", "stop"}:
            if args:
                raise ValueError(f"{call.group(1)} does not accept arguments")
            actions.append({"kind": "complete_quest" if name == "completequest" else "stop"})
        else:
            raise ValueError(
                f"minimal quest fragment {function_name} calls unsupported function {call.group(1)}"
            )
    if not actions:
        raise ValueError(f"minimal quest fragment {function_name} has no supported actions")
    return actions


def _minimal_quest_u16(value: str, function_name: str) -> int:
    try:
        parsed = int(value, 10)
    except ValueError as exc:
        raise ValueError(f"{function_name} index must be an integer literal") from exc
    if not 0 <= parsed <= 0xFFFF:
        raise ValueError(f"{function_name} index is outside u16 bounds")
    return parsed


def _minimal_quest_bool(value: str, function_name: str) -> bool:
    normalized = value.casefold()
    if normalized == "true":
        return True
    if normalized == "false":
        return False
    raise ValueError(f"{function_name} bool must be True or False")


def discover_skyrim_psc_artifacts(
    intents: Sequence[SkyrimScriptIntent],
    *,
    source_data_dir: Path | None,
    additional_source_asset_roots: Sequence[Path] = (),
    source_archives: Sequence[Path] = (),
    fo4_import_dirs: Sequence[Path],
    fo4_flags: str | None = None,
    temp_root: Path | None = None,
) -> SkyrimPapyrusProductionResult:
    intent_errors = _validate_intents(intents)
    if intent_errors:
        return SkyrimPapyrusProductionResult(False, (), (), "; ".join(intent_errors))

    roots = _dedupe_paths(
        Path(root)
        for root in (source_data_dir, *additional_source_asset_roots)
        if root is not None
    )
    archives = _discover_archives(roots, source_archives)
    loose_psc = _build_loose_index(roots, suffix=".psc")
    loose_pex = _build_loose_index(roots, suffix=".pex")
    archive_psc, archive_pex, archive_errors = _build_archive_indexes(archives)
    recovered: list[tuple[SkyrimPscSourceArtifact, SkyrimScriptReceipt]] = []
    required_failures: list[str] = []
    optional_failure_keys: set[str] = set()
    temp_parent = Path(temp_root) if temp_root is not None else None
    if temp_parent is not None:
        temp_parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="skyrim-papyrus-", dir=temp_parent) as temp_dir:
        work_dir = Path(temp_dir)
        for intent in intents:
            key = _script_key(intent.class_name)
            candidate = (
                loose_psc.get(key)
                or archive_psc.get(key)
                or loose_pex.get(key)
                or archive_pex.get(key)
            )
            if candidate is None:
                reason = "source PSC/PEX not found"
                if archive_errors:
                    reason += f" ({'; '.join(archive_errors)})"
                if intent.required:
                    required_failures.append(f"{intent.class_name}: {reason}")
                else:
                    optional_failure_keys.add(key)
                continue
            try:
                recovered.append(_recover_candidate(intent, candidate, work_dir))
            except Exception as exc:
                if intent.required:
                    required_failures.append(f"{intent.class_name}: {exc}")
                else:
                    optional_failure_keys.add(key)

        for artifact, reason in _validate_for_fo4(
            recovered,
            imports=fo4_import_dirs,
            flags=fo4_flags,
            work_dir=work_dir,
        ):
            if artifact.required:
                required_failures.append(f"{artifact.class_name}: {reason}")
            else:
                optional_failure_keys.add(_script_key(artifact.class_name))

    successful = [
        pair
        for pair in recovered
        if _script_key(pair[0].class_name) not in optional_failure_keys
        and not any(
            failure.lower().startswith(f"{pair[0].class_name.lower()}:")
            for failure in required_failures
        )
    ]
    receipts = tuple(receipt for _, receipt in successful)
    if required_failures:
        return SkyrimPapyrusProductionResult(
            False, (), receipts, "; ".join(required_failures)
        )
    return SkyrimPapyrusProductionResult(
        True,
        tuple(artifact for artifact, _ in successful),
        receipts,
        None,
    )


def _validate_intents(intents: Sequence[SkyrimScriptIntent]) -> list[str]:
    errors: list[str] = []
    seen: set[str] = set()
    for intent in intents:
        name = intent.class_name.strip()
        key = _script_key(name)
        if not name:
            errors.append("script class name is empty")
        elif len(name) > 38:
            errors.append(f"{name}: class name exceeds 38 characters")
        elif name[0].isdigit() or any(char not in _CLASS_NAME_CHARS for char in name):
            errors.append(f"{name}: invalid script class name")
        elif key in seen:
            errors.append(f"{name}: duplicate script class name")
        if not intent.kind.strip():
            errors.append(f"{name or '<empty>'}: script kind is empty")
        seen.add(key)
    return errors


def _recover_candidate(
    intent: SkyrimScriptIntent,
    candidate: _SourceCandidate,
    work_dir: Path,
) -> tuple[SkyrimPscSourceArtifact, SkyrimScriptReceipt]:
    raw = _read_candidate(candidate)
    if candidate.origin.endswith("psc"):
        source = _adapt_psc_source_for_fo4(_decode_psc(raw))
        adapters = ("skyrim-type-ast-v1",)
    else:
        pex_path = candidate.path
        if pex_path is None:
            pex_path = work_dir / "pex" / _script_relative_path(
                intent.class_name, ".pex"
            )
            pex_path.parent.mkdir(parents=True, exist_ok=True)
            pex_path.write_bytes(raw)
        source = _decompile_for_fo4(pex_path)
        adapters = (
            "skyrim-type-map-v1",
            "fo4-api-compat-v1",
            "skip-internal-functions-v1",
        )
    _validate_script_identity(intent.class_name, source)
    artifact = SkyrimPscSourceArtifact(
        intent.class_name, source, intent.kind, intent.required
    )
    receipt = SkyrimScriptReceipt(
        class_name=intent.class_name,
        origin=candidate.origin,
        container=str(candidate.container),
        member=candidate.member,
        input_sha256=hashlib.sha256(raw).hexdigest(),
        source_sha256=hashlib.sha256(source.encode("utf-8")).hexdigest(),
        adapters=adapters,
        fo4_validation="compiled",
    )
    return artifact, receipt


def _validate_for_fo4(
    recovered: Sequence[tuple[SkyrimPscSourceArtifact, SkyrimScriptReceipt]],
    *,
    imports: Sequence[Path],
    flags: str | None,
    work_dir: Path,
) -> list[tuple[SkyrimPscSourceArtifact, str]]:
    source_root = work_dir / "fo4-source"
    for artifact, _ in recovered:
        source_path = source_root / _script_relative_path(artifact.class_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(artifact.source, encoding="utf-8")

    import_args = [str(path) for path in _dedupe_paths((source_root, *imports))]
    failures: list[tuple[SkyrimPscSourceArtifact, str]] = []
    for artifact, _ in recovered:
        source_path = source_root / _script_relative_path(artifact.class_name, ".psc")
        try:
            result = _compile_fo4_source(
                artifact.source,
                imports=import_args,
                flags=flags,
                source_path=str(source_path),
            )
        except Exception as exc:
            failures.append((artifact, f"FO4 validation failed: {exc}"))
            continue
        if not result.ok or result.pex_bytes is None:
            diagnostics = ", ".join(
                str(item.get("message", item))
                for item in getattr(result, "diagnostics", ())
            )
            failures.append(
                (artifact, f"FO4 validation failed: {diagnostics or 'compile failed'}")
            )
    return failures


def _adapt_psc_source_for_fo4(source: str) -> str:
    from creation_lib.papyrus_lsp import parse_script
    from creation_lib.papyrus_lsp.native_runtime import emit_script_native

    parsed = parse_script(text=source)
    if parsed.ast is None or parsed.errors:
        detail = ", ".join(
            f"line {error.line}: {error.message}" for error in parsed.errors
        )
        raise ValueError(f"PSC parse failed: {detail or 'no syntax tree'}")
    _adapt_ast_types(parsed.ast)
    return emit_script_native(parsed.ast)


def _adapt_ast_types(value: Any) -> None:
    if is_dataclass(value):
        for field in fields(value):
            child = getattr(value, field.name)
            if field.name in _TYPE_FIELDS and isinstance(child, str):
                setattr(value, field.name, skyrim_type_for_fo4(child))
            else:
                _adapt_ast_types(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _adapt_ast_types(child)


def _decompile_for_fo4(pex_path: Path) -> str:
    from creation_lib.pex import decompile_pex

    return decompile_pex(
        pex_path,
        type_adapter=skyrim_type_for_fo4,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )


def _compile_fo4_source(
    source: str,
    *,
    imports: list[str],
    flags: str | None,
    source_path: str,
) -> Any:
    from creation_lib.pex.native_runtime import compile_psc

    return compile_psc(
        source,
        imports=imports,
        game="fo4",
        flags=flags,
        source_path=source_path,
    )


def _validate_script_identity(class_name: str, source: str) -> None:
    from creation_lib.papyrus_lsp import parse_script

    parsed = parse_script(text=source)
    if parsed.ast is None or parsed.errors:
        detail = ", ".join(
            f"line {error.line}: {error.message}" for error in parsed.errors
        )
        raise ValueError(f"recovered PSC parse failed: {detail or 'no syntax tree'}")
    if _script_key(parsed.ast.name) != _script_key(class_name):
        raise ValueError(
            f"recovered ScriptName {parsed.ast.name!r} does not match {class_name!r}"
        )


def _read_candidate(candidate: _SourceCandidate) -> bytes:
    if candidate.path is not None:
        return candidate.path.read_bytes()
    if candidate.member is None:
        raise ValueError("archive candidate has no member")
    from creation_lib.ba2.native_runtime import extract_one

    raw = extract_one(str(candidate.container), candidate.member)
    if raw is None:
        raise ValueError(
            f"could not extract {candidate.member} from {candidate.container}"
        )
    return bytes(raw)


def _decode_psc(raw: bytes) -> str:
    try:
        return raw.decode("utf-8-sig")
    except UnicodeDecodeError:
        return raw.decode("cp1252")


def _build_loose_index(
    roots: Sequence[Path], *, suffix: str
) -> dict[str, _SourceCandidate]:
    index: dict[str, _SourceCandidate] = {}
    for root in _loose_script_roots(roots, suffix=suffix):
        if not root.is_dir():
            continue
        for path in root.rglob(f"*{suffix}"):
            relative = path.relative_to(root).with_suffix("")
            class_name = ":".join(_trim_script_prefix(relative.parts))
            if class_name:
                index.setdefault(
                    _script_key(class_name),
                    _SourceCandidate(
                        origin=f"loose-{suffix[1:]}",
                        container=path,
                        path=path,
                    ),
                )
    return index


def _loose_script_roots(roots: Sequence[Path], *, suffix: str) -> list[Path]:
    candidates: list[Path] = []
    for root in roots:
        if suffix == ".psc":
            candidates.extend(
                (
                    root / "Scripts" / "Source" / "User",
                    root / "Scripts" / "Source",
                    root / "Source" / "Scripts",
                    root / "Data" / "Scripts" / "Source" / "User",
                    root / "Data" / "Scripts" / "Source",
                    root / "Data" / "Source" / "Scripts",
                )
            )
            if root.name.lower() == "scripts":
                candidates.extend((root / "Source" / "User", root / "Source"))
            if root.name.lower() == "source":
                candidates.append(root)
        else:
            candidates.extend(
                (
                    root / "Scripts" / "Client",
                    root / "Scripts",
                    root / "scripts" / "client",
                    root / "Data" / "Scripts",
                )
            )
            if root.name.lower() == "scripts":
                candidates.append(root)
    return _dedupe_paths(candidates)


def _discover_archives(
    roots: Sequence[Path], explicit_archives: Sequence[Path]
) -> list[Path]:
    archives = [Path(path) for path in explicit_archives]
    for root in roots:
        if root.is_dir():
            archives.extend(
                sorted(
                    (
                        path
                        for path in root.iterdir()
                        if path.is_file() and path.suffix.lower() == ".bsa"
                    ),
                    key=lambda path: path.name.lower(),
                )
            )
    return _dedupe_paths(archives)


def _build_archive_indexes(
    archives: Sequence[Path],
) -> tuple[
    dict[str, _SourceCandidate],
    dict[str, _SourceCandidate],
    list[str],
]:
    from creation_lib.ba2.native_runtime import list_archive

    psc_index: dict[str, _SourceCandidate] = {}
    pex_index: dict[str, _SourceCandidate] = {}
    errors: list[str] = []
    for archive in archives:
        try:
            members = list_archive(str(archive)) or []
        except Exception as exc:
            errors.append(f"archive {archive}: could not list: {exc}")
            continue
        for member in members:
            class_name, suffix = _class_name_from_archive_member(str(member))
            if class_name is None or suffix is None:
                continue
            candidate = _SourceCandidate(
                origin=f"archive-{suffix[1:]}",
                container=archive,
                member=str(member),
            )
            target = psc_index if suffix == ".psc" else pex_index
            target.setdefault(_script_key(class_name), candidate)
    return psc_index, pex_index, errors


def _class_name_from_archive_member(member: str) -> tuple[str | None, str | None]:
    parts = tuple(
        part for part in member.replace("\\", "/").strip("/").split("/") if part
    )
    lowered = tuple(part.lower() for part in parts)
    suffix = Path(parts[-1]).suffix.lower() if parts else ""
    if suffix == ".psc":
        for prefix in (
            ("scripts", "source", "user"),
            ("scripts", "source"),
            ("source", "scripts"),
        ):
            if lowered[: len(prefix)] == prefix:
                relative = parts[len(prefix) :]
                break
        else:
            return None, None
    elif suffix == ".pex" and lowered[:1] == ("scripts",):
        relative = parts[1:]
    else:
        return None, None
    relative = _trim_script_prefix(relative)
    if not relative:
        return None, None
    relative = (*relative[:-1], Path(relative[-1]).stem)
    return ":".join(relative), suffix


def _trim_script_prefix(parts: Sequence[str]) -> tuple[str, ...]:
    trimmed = tuple(parts)
    while trimmed and trimmed[0].lower() in {"base", "client", "server", "user"}:
        trimmed = trimmed[1:]
    return trimmed


def _script_relative_path(class_name: str, suffix: str) -> Path:
    normalized = class_name.replace("\\", ":").replace("/", ":")
    parts = [part for part in normalized.split(":") if part]
    return Path(*parts).with_suffix(suffix)


def _script_key(class_name: str) -> str:
    return class_name.replace("\\", ":").replace("/", ":").strip().lower()


def _dedupe_paths(paths: Any) -> list[Path]:
    seen: set[str] = set()
    result: list[Path] = []
    for path in paths:
        candidate = Path(path)
        key = str(candidate).replace("\\", "/").lower()
        if key not in seen:
            seen.add(key)
            result.append(candidate)
    return result
