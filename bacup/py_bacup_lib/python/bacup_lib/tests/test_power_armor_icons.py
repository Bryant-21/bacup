import json
from pathlib import Path

import numpy as np
from bacup_lib.power_armor_icons import (LINE_ALPHA, MODELS, SIZE, facing_front, front_icon, list_icon, model_triangles,
                                         pitched, runs, turned)
from creation_lib.nif.nif_file import NifFile


def quad(x0, x1, z0, z1, depth):
    return [[[x0, depth, z0], [x1, depth, z0], [x1, depth, z1]],
            [[x0, depth, z0], [x1, depth, z1], [x0, depth, z1]]]


def test_front_camera_shows_front_detail_and_hides_back_detail():
    body = quad(-10, 10, -10, 10, 0)
    front = front_icon(np.array(body + quad(-4, 4, -4, 4, 4), dtype=float))
    back = front_icon(np.array(body + quad(-4, 4, -4, 4, -4), dtype=float))
    assert np.count_nonzero(front != back) > 100
    assert np.any(front == LINE_ALPHA)
    assert np.all(back[SIZE//2, SIZE//2-40:SIZE//2+40] == 255)
    assert np.array_equal(back, np.flip(back, axis=1))
    assert not np.any(front[:8]) and not np.any(front[-8:])
    assert not np.any(front[:, :8]) and not np.any(front[:, -8:])


def test_run_encoding_reconstructs_every_alpha_pixel():
    image = front_icon(np.array(quad(-5, 5, -10, 10, 0), dtype=float))
    decoded = np.zeros((SIZE, SIZE), dtype=np.uint8)
    for x, y, length, alpha in runs(image):
        decoded[y, x:x+length] = alpha
    assert np.array_equal(image, decoded)
    assert len(runs(image)) < SIZE*10


def test_model_table_names_each_family_piece_once():
    entries = json.loads(MODELS.read_text())
    keys = [(entry["keyword"], entry["piece"]) for entry in entries]
    assert len(keys) == len(set(keys))
    assert {entry["piece"] for entry in entries} == {"Helmet", "Torso", "LeftArm", "RightArm", "LeftLeg", "RightLeg"}
    assert all(entry["piece"].startswith(("Left", "Right")) for entry in entries if entry.get("mirror"))
    assert all(entry["piece"] == "Helmet" for entry in entries if "eyes" in entry)


def test_a_limb_model_shared_by_both_sides_is_mirrored_on_one():
    entries = json.loads(MODELS.read_text())
    for limb in ("Arm", "Leg"):
        families: dict[str, dict] = {}
        for entry in entries:
            if entry["piece"].endswith(limb):
                families.setdefault(entry["keyword"], {})[entry["piece"].removesuffix(limb)] = entry
        for sides in families.values():
            if sides["Left"]["model"].lower() == sides["Right"]["model"].lower():
                assert sides["Left"].get("mirror", False) != sides["Right"].get("mirror", False), sides["Left"]["name"]


def test_front_camera_puts_the_wearers_right_on_the_left():
    # A body with a pad on its right shoulder (+x) only.
    body = np.array(quad(-5, 5, -10, 10, 0) + quad(5, 15, 5, 10, 0), dtype=float)
    icon = front_icon(facing_front(body))
    rows, columns = np.nonzero(icon)
    left = columns.min()
    assert icon[rows.min() + 2, left + 2] > 0
    assert icon[rows.max() - 2, left + 2] == 0


class Block:
    def __init__(self, **fields):
        self.fields = fields

    def get_field(self, name):
        return self.fields.get(name)


def test_node_rotation_turns_children_as_fo4_does(monkeypatch):
    # NiMatrix3 entries by name. FO4 rotates row vectors (p·M), so this turns +x to +y.
    turn = {"m11": 0, "m12": 1, "m13": 0, "m21": -1, "m22": 0, "m23": 0, "m31": 0, "m32": 0, "m33": 1}
    vertices = [{"Vertex": dict(zip("xyz", point))} for point in ((1, 0, 0), (1, 0, 1), (2, 0, 0))]
    blocks = [Block(Rotation=turn, Children=[1]),
              Block(**{"Vertex Data": vertices, "Triangles": [{"v1": 0, "v2": 1, "v3": 2}]})]
    nif = type("Nif", (), {"get_block": lambda self, index: blocks[index]})()
    monkeypatch.setattr(NifFile, "load", staticmethod(lambda path: nif))
    assert np.allclose(model_triangles(Path("model.nif")), [[[0, 1, 0], [0, 1, 1], [0, 2, 0]]])


def test_turn_faces_a_sideways_model_forward():
    side = np.array(quad(-10, 10, -10, 10, 0), dtype=float)[..., [1, 0, 2]]
    assert np.allclose(turned(side, 90)[..., 1], 0)
    assert np.array_equal(front_icon(turned(side, 90)), front_icon(np.array(quad(-10, 10, -10, 10, 0), dtype=float)))


def test_pitch_stands_up_a_model_lying_on_its_back():
    standing = np.array(quad(-10, 10, -10, 4, 0), dtype=float)
    lying = standing[..., [0, 2, 1]]
    assert np.array_equal(front_icon(pitched(lying, 90)), front_icon(standing))


def test_list_icon_is_a_solid_silhouette_without_detail_lines():
    body = np.array(quad(-10, 10, -10, 10, 0) + quad(-4, 4, -4, 4, 4), dtype=float)
    icon = list_icon(body)
    assert np.all(icon[SIZE//2, SIZE//4:3*SIZE//4] == 255)
    assert not np.any((icon > 0) & (icon < 255) & (np.arange(SIZE)[:, None] > 20) & (np.arange(SIZE)[:, None] < SIZE-20)
                      & (np.arange(SIZE)[None, :] > 20) & (np.arange(SIZE)[None, :] < SIZE-20))


def test_list_icon_cuts_eyes_that_read_at_text_height():
    face = np.array(quad(-10, 10, -10, 10, 0), dtype=float)
    slit = [[.3, .3], [.45, .3], [.45, .31], [.3, .31]]
    icon = list_icon(face, [slit, [[1 - x, y] for x, y in slit]])
    left, right = icon[:, :SIZE//2], icon[:, SIZE//2:]
    assert np.array_equal(left, np.flip(right, axis=1))
    rows = np.nonzero((icon[:, SIZE//2-30:SIZE//2-14] == 0).any(axis=1) & (np.arange(SIZE) > 20) & (np.arange(SIZE) < SIZE-20))[0]
    assert rows.size >= .1 * (SIZE - 32)
    assert np.all(icon[SIZE-30, 30:SIZE-30] == 255)
