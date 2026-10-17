from __future__ import annotations

import json
import struct
from contextlib import contextmanager
from pathlib import Path
from types import SimpleNamespace

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.run import ConversionRun
from bacup_lib.timing_report import TimingReport
from bacup_lib.workflows import unified
from creation_lib.esp import Plugin
from creation_lib.esp.model import Record, Subrecord
from creation_lib.pex.native_runtime import compile_psc, parse_pex_file_native


def script_path(name: str, suffix: str = ".psc") -> Path:
    return Path(*name.split(":")).with_suffix(suffix)


def write_pex(path: Path, name: str) -> None:
    result = compile_psc(f"Scriptname {name}\n", game="fo4")
    assert result.ok, result.diagnostics
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(result.pex_bytes)


def assert_pex(path: Path, name: str) -> None:
    parsed = parse_pex_file_native(path)
    assert [obj.name for obj in parsed.objects] == [name]


class NativeScripts:
    def __init__(self, root: Path, refs=(), extra_subrecords=None):
        self.root = root
        self.resources = root / "resources"
        self.headers = root / "headers"
        self.source = root / "source"
        self.target = root / "target"
        for path in (self.resources, self.headers, self.source, self.target):
            path.mkdir(parents=True, exist_ok=True)
        target_plugin = root / "Target.esp"
        with Plugin.new(target_plugin.name, game="fo4") as plugin:
            for i, (name, signature) in enumerate(refs, 0x800):
                encoded = name.encode("utf-8")
                vmad = struct.pack("<HHHH", 6, 2, 1, len(encoded)) + encoded + b"\0\0\0"
                plugin.add_record(
                    Record(
                        signature=signature,
                        form_id=i,
                        form_version=131,
                        subrecords=[
                            Subrecord("EDID", f"Fixture{i:06X}\0".encode()),
                            Subrecord("VMAD", vmad),
                            *(extra_subrecords or {}).get(i, ()),
                        ],
                    )
                )
            plugin.save(target_plugin)
        self.run = ConversionRun.open_existing("fo76", "fo4", None, str(target_plugin))
        self.runtime = unified._UnifiedRecordRuntime(
            PluginPortRequest(
                source_game="fo76",
                target_game="fo4",
                source_plugins=[],
                output_root=root / "out",
                source_data_dir=self.source,
                target_data_dir=self.target,
                target_extracted_dir=None,
                options=PluginPortOptions(
                    papyrus_compiler="native", conversion_workers=2
                ),
            )
        )
        self.ctx = SimpleNamespace(
            _rust_conversion_run=self,
            mod_path=root / "mod",
            diagnostics_root=root / "diagnostics",
            summary=SimpleNamespace(scripts_flagged=0),
            timing_report=TimingReport(),
        )
        self.dispatches = []

    @property
    def id(self):
        return self.run.id

    def run_phase(self, name, **kwargs):
        self.dispatches.append((name, kwargs))
        kwargs["params"] = {
            **kwargs["params"],
            "resources_root": str(self.resources),
            "bundled_corpus_root": str(self.headers),
        }
        return self.run.run_phase(name, **kwargs)

    def addition(self, name, source=None):
        path = self.resources / "script_additions/fo76_fo4" / script_path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source or f"Scriptname {name}\n", encoding="utf-8")
        return path

    def convert(self):
        self.runtime._run_convert_scripts_phase(
            self.ctx, SimpleNamespace(emit_log=lambda *_: None)
        )
        assert [name for name, _ in self.dispatches] == ["convert_scripts"]
        return json.loads(
            (self.ctx.diagnostics_root / "script_port_report.json").read_text()
        )

    def psc(self, name):
        return self.ctx.mod_path / "Scripts/Source/User" / script_path(name)

    def pex(self, name):
        return self.ctx.mod_path / "data/Scripts" / script_path(name, ".pex")

    def references(self):
        return unified.load_native_module().conversion_run_inspect_script_references(
            self.id
        )[1]

    def saved_subrecords(self, form_id, signature):
        from creation_lib.esp.native_runtime import plugin_handle_record_subrecords

        path = self.root / "Saved.esp"
        self.run.save_target(str(path), run_nvnm_validator=False)
        with Plugin.load(path, game="fo4") as plugin:
            return [
                data
                for sig, data, _ in plugin_handle_record_subrecords(
                    plugin._rust_handle, form_id
                )
                if sig == signature
            ]


@contextmanager
def native_scripts(root: Path, refs=(), extra_subrecords=None):
    fixture = NativeScripts(root, refs, extra_subrecords)
    try:
        yield fixture
    finally:
        fixture.run.close()
