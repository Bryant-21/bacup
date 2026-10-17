from __future__ import annotations

import base64
import functools
import json
import logging
import zlib
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable

import numpy as np

from bacup_lib.native_runtime import load_native_module

SIZE = 128
HIDDEN = 0x1
REFRACTION = 1 << 15
ELEVATION = 20
HUMAN_RACE = "013746:Fallout4.esm"
KEYCARD = "props/generickeycard01.nif"
CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/item_icons.json")
ENCODING = "zlib-alpha8"
FO4_MASTERS = ("Fallout4.esm", "DLCRobot.esm", "DLCworkshop01.esm", "DLCCoast.esm",
               "DLCworkshop02.esm", "DLCworkshop03.esm", "DLCNukaWorld.esm")
_log = logging.getLogger(__name__)


@dataclass(frozen=True)
class Item:
    form: str
    signature: str
    editor_id: str
    name: str
    model: str
    plugin: str
    playable: bool
    thrown: bool = False
    addons: tuple[str, ...] = ()
    worn: tuple[str, ...] = ()


def _unpack_item(value: dict) -> Item:
    return Item(**{**value, "addons": tuple(value["addons"]), "worn": tuple(value["worn"])})


def _item(raw: int, record: dict, signature: str, plugin: str, masters: list[str]) -> Item:
    return _unpack_item(json.loads(load_native_module().item_icon_record(
        raw, json.dumps(record), signature, plugin, masters)))


def icon_model(item: Item) -> str | None:
    return load_native_module().item_icon_model(json.dumps(asdict(item)))


def worn_items(items: Iterable[Item], addons: dict[str, tuple[str, str]]) -> list[Item]:
    return [_unpack_item(value) for value in json.loads(load_native_module().item_icon_worn(
        json.dumps([asdict(item) for item in items]), json.dumps(addons)))]


@dataclass(frozen=True)
class GameData:
    """Where the game finds a data file ("meshes/x.nif"): loose files in `directories`, highest priority first, then
    the BA2 archives in `archive_directories`. A conversion's own assets are still loose when its icons render."""
    directories: tuple[str, ...]
    archive_directories: tuple[str, ...] = ()

    def loose(self, name: str) -> Path | None:
        return next((path for root in self.directories if (path := Path(root) / name).is_file()), None)

    def exists(self, name: str) -> bool:
        return self.loose(name) is not None or bool(self.archive_directories) and _archives(
            self.archive_directories).has_file(name)

    def read(self, name: str) -> bytes | None:
        if path := self.loose(name):
            return path.read_bytes()
        return _archives(self.archive_directories).find(name) if self.archive_directories else None


@functools.cache
def _archives(directories: tuple[str, ...]):
    from creation_lib.ba2.ba2_manager import BA2Manager

    manager = BA2Manager()
    manager.scan_directories([Path(directory) for directory in directories])
    return manager


def model_file(data: GameData, model: str) -> str | None:
    return load_native_module().item_icon_model_file(model, json.dumps(asdict(data)))


def master_strings(data: GameData, plugins: Iterable[Path], directory: Path) -> Path:
    """Copies each plugin's English string tables to `directory`; FO4 keeps its masters' tables in their BA2s."""
    directory.mkdir(parents=True, exist_ok=True)
    for plugin in plugins:
        for suffix in ("strings", "dlstrings", "ilstrings"):
            if (raw := data.read(f"strings/{plugin.stem}_en.{suffix}")) is not None:
                (directory / f"{plugin.stem}_en.{suffix}").write_bytes(raw)
    return directory


def _triangles(triangles: np.ndarray) -> bytes:
    return np.ascontiguousarray(triangles, dtype="<f8").tobytes()


def view_basis(triangles: np.ndarray, signature: str) -> tuple[str, np.ndarray]:
    view, basis = json.loads(load_native_module().item_icon_view(_triangles(triangles), signature))
    return view, np.asarray(basis)


def item_icon(triangles: np.ndarray, signature: str) -> tuple[str, np.ndarray]:
    view, alpha = load_native_module().render_item_icon(_triangles(triangles), signature)
    return view, np.frombuffer(alpha, dtype=np.uint8).reshape(SIZE, SIZE)


def visible_triangles(nif, data: GameData = GameData(())) -> tuple[np.ndarray, list[tuple[str, str]]]:
    payload = [(block.type_name, block.to_json()) for block in nif.blocks]
    triangles, skipped = load_native_module().item_icon_geometry(payload, json.dumps(asdict(data)))
    return np.frombuffer(triangles, dtype="<f8").reshape(-1, 3, 3), [tuple(value) for value in json.loads(skipped)]


def _texture_alpha(name: str, data: GameData) -> np.ndarray | None:
    result = load_native_module().item_icon_texture_alpha(name, json.dumps(asdict(data)))
    if result is None:
        return None
    width, height, alpha = result
    return np.frombuffer(alpha, dtype=np.uint8).reshape(height, width)


def alpha_tested_triangles(points: np.ndarray, uvs: np.ndarray, faces: np.ndarray, texture: np.ndarray,
                           vertex_alpha: np.ndarray, threshold: float, pixel: float) -> np.ndarray:
    height, width = texture.shape
    triangles = load_native_module().item_icon_alpha_cut(
        points.tolist(), uvs.tolist(), faces.tolist(), np.ascontiguousarray(texture, dtype=np.uint8).tobytes(),
        width, height, vertex_alpha.tolist(), threshold, pixel)
    return np.frombuffer(triangles, dtype="<f8").reshape(-1, 3, 3)


def encoded(alpha: np.ndarray) -> str:
    return base64.b64encode(zlib.compress(np.ascontiguousarray(alpha, dtype=np.uint8).tobytes(), 9)).decode("ascii")


def decoded(text: str) -> np.ndarray:
    return np.frombuffer(zlib.decompress(base64.b64decode(text)), dtype=np.uint8).reshape(SIZE, SIZE)


def icon_flags(alpha: np.ndarray) -> list[str]:
    return load_native_module().item_icon_flags(np.ascontiguousarray(alpha, dtype=np.uint8).tobytes())


def generate_item_icons(plugins: Iterable[tuple[Path, Path | None]], data: GameData, mod_root: Path,
                        *, workers: int | None = None, report: Path | None = None) -> Path:
    request = {"plugins": [(str(plugin), str(strings) if strings else None) for plugin, strings in plugins],
               "data": asdict(data), "mod_root": str(mod_root), "workers": workers,
               "report": str(report) if report else None}
    result = json.loads(load_native_module().generate_item_icons(json.dumps(request)))
    _log.info("Generated %d item icons for %d primary and %d fallback forms of %d",
              result["icons"], result["primary_forms"], result["fallback_forms"], result["eligible_forms"])
    _log.info("[item_icon_timing] %s", json.dumps(result["timings"], sort_keys=True))
    return Path(result["output"])
