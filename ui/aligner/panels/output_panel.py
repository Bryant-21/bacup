"""Output panel — displays YAML values for scope alignment and copy-to-clipboard."""
from __future__ import annotations

import numpy as np
from imgui_bundle import imgui


class OutputPanel:
    """Read-only YAML output of current scope alignment values."""

    def __init__(self, app):
        self._app = app
        self.window_name = "Output##aligner"

    def draw(self):
        camera = self._app.camera

        imgui.begin(self.window_name)

        imgui.text("YAML Values")
        imgui.separator()
        imgui.spacing()

        yaml_text = (
            f"ZoomDataCameraOffsetX: {camera.offset_x:.4f}\n"
            f"ZoomDataCameraOffsetY: {camera.offset_y:.4f}\n"
            f"ZoomDataCameraOffsetZ: {camera.offset_z:.4f}\n"
            f"ZoomDataFOVMult: {camera.fov_mult:.4f}"
        )

        imgui.input_text_multiline(
            "##yaml_output", yaml_text, imgui.ImVec2(-1, 120),
            imgui.InputTextFlags_.read_only.value,
        )

        imgui.spacing()
        if imgui.button("Copy to Clipboard"):
            imgui.set_clipboard_text(yaml_text)

        # --- Bone World Positions (for comparison with 3ds Max) ---
        imgui.spacing()
        imgui.separator()
        imgui.spacing()
        imgui.text("Bone World Transforms")

        skel = self._app._skeleton_data
        if skel:
            cx, cy, cz = skel["camera_pos"]
            wx, wy, wz = skel["weapon_pos"]

            imgui.text(f"Camera:  X={cx:9.4f}  Y={cy:9.4f}  Z={cz:9.4f}")
            imgui.text(f"Weapon:  X={wx:9.4f}  Y={wy:9.4f}  Z={wz:9.4f}")

            fx = cx + camera.offset_x
            fy = cy + camera.offset_y
            fz = cz + camera.offset_z
            imgui.text(f"Cam+Ofs: X={fx:9.4f}  Y={fy:9.4f}  Z={fz:9.4f}")

            dx, dy, dz = fx - wx, fy - wy, fz - wz
            imgui.text(f"Delta:   X={dx:9.4f}  Y={dy:9.4f}  Z={dz:9.4f}")

            # Weapon bone rotation (Euler angles for easy Max comparison)
            weapon_rot = skel.get("weapon_rot")
            if weapon_rot is not None:
                r = weapon_rot
                # Extract Euler angles (XYZ order) from 3x3 rotation matrix
                sy = np.sqrt(r[0, 0] ** 2 + r[1, 0] ** 2)
                if sy > 1e-6:
                    ex = np.degrees(np.arctan2(r[2, 1], r[2, 2]))
                    ey = np.degrees(np.arctan2(-r[2, 0], sy))
                    ez = np.degrees(np.arctan2(r[1, 0], r[0, 0]))
                else:
                    ex = np.degrees(np.arctan2(-r[1, 2], r[1, 1]))
                    ey = np.degrees(np.arctan2(-r[2, 0], sy))
                    ez = 0.0
                imgui.text(f"Wpn Rot: X={ex:7.2f}  Y={ey:7.2f}  Z={ez:7.2f} deg")
        else:
            imgui.text_disabled("Load animation to see bone positions")

        # World positions for additional bones
        positions = self._app._world_positions
        if positions:
            if imgui.tree_node("All Bone Positions"):
                for name in ["Root", "COM", "Pelvis", "Spine1", "Spine2", "Chest",
                             "RArm_Collarbone", "RArm_UpperArm", "RArm_ForeArm1",
                             "RArm_ForeArm2", "RArm_ForeArm3", "RArm_Hand",
                             "Weapon", "WeaponBolt", "WeaponOptics1",
                             "Camera", "Camera Control",
                             "LArm_Hand", "Head", "Neck"]:
                    if name in positions:
                        px, py, pz = positions[name]
                        imgui.text(f"{name:24s} ({px:9.4f}, {py:9.4f}, {pz:9.4f})")
                imgui.tree_pop()

        imgui.end()
