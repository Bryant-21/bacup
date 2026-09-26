from __future__ import annotations

import re
from collections.abc import Iterable


def resolve_numbered_name(names: Iterable[str], stem: str, description: str) -> str:
    pattern = re.compile(re.escape(stem) + r"\d+\Z")
    matches = sorted({name for name in names if pattern.fullmatch(name)})
    if len(matches) != 1:
        detail = "none" if not matches else ", ".join(matches)
        raise ValueError(f"Expected one {description} matching {stem}<number>; found {detail}")
    return matches[0]


def render_as3_template(source: str, replacements: dict[str, str]) -> str:
    for token, value in replacements.items():
        if token not in source:
            raise RuntimeError(f"ActionScript template is missing {token}")
        source = source.replace(token, value)
    return source


def resolve_numbered_as3_call(
    native_runtime,
    movie: bytes,
    class_name: str,
    method_name: str,
    stem: str,
    description: str,
) -> str:
    method_names = {method_name, method_name.removeprefix("$")}
    methods = [
        method
        for method in native_runtime.abc_disassemble(movie, class_name)
        if method["method"] in method_names
    ]
    if len(methods) != 1:
        raise ValueError(f"Expected one {class_name}.{method_name} method")
    calls = []
    for instruction in methods[0]["code"]:
        match = re.search(r"\bCallPropVoid\s+(\S+)\s+\(\d+\)", instruction)
        if match:
            calls.append(match.group(1))
    return resolve_numbered_name(calls, stem, description)
