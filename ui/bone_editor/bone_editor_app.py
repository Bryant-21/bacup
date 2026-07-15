"""Bone Editor — orchestrator.

Holds skeleton, PoseSession, viewport rendering state, and the panels.
The PoseSession is the single source of truth for all edits; both the
viewport preview and the apply pipeline read from it.
"""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Optional

import moderngl

from creation_lib.bone_edit.bone_classifier import BoneClassifier
from creation_lib.bone_edit.skeleton import SkeletonManager
from ui.aligner.scope_camera import ScopeCamera
from ui.editor.nif_session import NifRegistry
from creation_lib.renderer.scene_renderer import SceneRenderer
from creation_lib.renderer.lighting import LightingSetup

# TYPE import only; the dependency is not called in production flow.
# These dataclasses define the shape that sample_clip_to_world_frames consumes.
from creation_lib.animation.models import AnimationClip, AnimationEvent, AnimationKeyframe, BoneChannel

_log = logging.getLogger("bone_editor.app")


# ---------------------------------------------------------------------------
# Native JSON → AnimationClip adapter
# ---------------------------------------------------------------------------

def _animation_clip_from_native_data(
    data: dict | str,
    bone_names: list[str] | None = None,
) -> AnimationClip | None:
    """Rebuild a frozen AnimationClip dataclass tree from havok_extract_clip output.

    Accepts either the dict returned by creation_lib.havok.native_runtime.extract_clip_native
    or the raw JSON string produced by havok_native.havok_extract_clip. Fields
    absent from the Rust struct (name, cycle_type, frequency, accum_root,
    float_channels, is_additive, track_to_bone_indices) receive their Python
    dataclass defaults — none of those fields are read by sample_clip_to_world_frames.

    bone_names: when provided, channels named track_N (Rust output when no
    skeleton XML is available) are renamed using bone index N from this list,
    matching the Python extract_clip behaviour.
    """
    if isinstance(data, str):
        try:
            data = json.loads(data)
        except (json.JSONDecodeError, TypeError):
            return None

    if not data:
        return None

    # Build bone channel objects
    channels = []
    for ch in data.get("channels", []):
        bone_name = ch.get("bone_name", "")

        # Rename track_N → actual bone name when the caller supplies a name list
        if bone_names is not None and bone_name.startswith("track_"):
            try:
                idx = int(bone_name[len("track_"):])
                if 0 <= idx < len(bone_names):
                    bone_name = bone_names[idx]
            except ValueError:
                pass

        rotations = tuple(
            AnimationKeyframe(time=kf["time"], value=tuple(kf["value"]))
            for kf in ch.get("rotations", [])
        )
        translations = tuple(
            AnimationKeyframe(time=kf["time"], value=tuple(kf["value"]))
            for kf in ch.get("translations", [])
        )
        scales = tuple(
            AnimationKeyframe(time=kf["time"], value=tuple(kf["value"]))
            for kf in ch.get("scales", [])
        )
        channels.append(BoneChannel(
            bone_name=bone_name,
            rotations=rotations,
            translations=translations,
            scales=scales,
        ))

    events = tuple(
        AnimationEvent(time=ev["time"], text=ev.get("text", ""))
        for ev in data.get("events", [])
    )

    original_skeleton_name = data.get("original_skeleton_name") or ""

    return AnimationClip(
        name=original_skeleton_name,
        duration=float(data.get("duration", 0.0)),
        source_format=data.get("source_format", "hkx"),
        native_fps=float(data.get("native_fps", 30.0)),
        channels=tuple(channels),
        events=events,
        warnings=tuple(data.get("warnings", [])),
        original_skeleton_name=original_skeleton_name,
    )


def _infer_clip_fps(clip: AnimationClip, default: float = 30.0) -> float:
    """Return native_fps when already computed by the Rust extractor.

    Falls back to deriving fps from keyframe spacing (same logic as Python's
    infer_clip_fps in creation_lib.havok.animation_reader) for clips where native_fps
    was not populated.
    """
    if clip.native_fps and clip.native_fps > 0.0:
        return clip.native_fps
    for ch in clip.channels:
        for series in (ch.rotations, ch.translations, ch.scales):
            if len(series) >= 2:
                dt = series[1].time - series[0].time
                if dt > 1e-6:
                    return 1.0 / dt
    return default


class BoneEditorApp:
    def __init__(self, toolkit_settings=None):
        self.ctx: Optional[moderngl.Context] = None
        self.renderer: Optional[SceneRenderer] = None
        self.render_mode_mgr = None  # SceneRenderer reads this; None → TEXTURED
        self.camera = ScopeCamera()
        self.lighting = LightingSetup()
        self.registry = NifRegistry()
        self.toolkit_settings = toolkit_settings
        self.status_text = "Load a skeleton HKX to begin"
        self.active = True

        # Engine state
        self.skeleton: Optional[SkeletonManager] = None
        self._skeleton_nif_path: Optional[str] = None
        self.classifier_hidden: set[str] = set()
        self.pose_session = None  # PoseSession, set when skeleton loads

        # Skinned mesh
        self.skinned_renderer = None
        self.skinned_meshes: list = []
        self.ba2_manager = None

        # Panels (created in _init_panels)
        self.setup_panel = None
        self.bone_panel = None
        self.apply_panel = None
        self.viewport_panel = None
        self.viewport_interact = None

        # Animation playback — owns frame list and transport state.
        # Hooked to pose_session inside load_skeleton() so it can write
        # `playback_pose` / `_playback_active` during update().
        from .playback_controller import PlaybackController
        self.playback = PlaybackController()

        self._first_frame = True
        self._panels_initialized = False

    def setup(self) -> None:
        """Called once when the GL context is available."""
        self.ctx = moderngl.get_context()
        self.renderer = SceneRenderer(self.ctx)
        self.renderer.init_shaders()
        self.renderer.init_grid()

        from .skinned_renderer import SkinnedRenderer
        self.skinned_renderer = SkinnedRenderer(self.ctx)
        _log.info("Bone editor GL initialized: %s", self.ctx.info["GL_RENDERER"])

    def _init_panels(self) -> None:
        if self._panels_initialized:
            return
        from .panels.apply_panel import ApplyPanel
        from .panels.bone_panel import BonePanel
        from .panels.setup_panel import SetupPanel
        from .viewport_panel import ViewportPanel

        self.setup_panel = SetupPanel(self)
        self.bone_panel = BonePanel(self)
        self.apply_panel = ApplyPanel(self)
        self.viewport_panel = ViewportPanel(self)
        self._panels_initialized = True

    # ──────────────────────────────────────────────────────────
    # Loading
    # ──────────────────────────────────────────────────────────

    def load_skeleton(self, hkx_path: str, nif_path: Optional[str] = None) -> None:
        from .pose_session import PoseSession

        self.skeleton = SkeletonManager.from_hkx(Path(hkx_path))

        # Optionally augment with NIF skeleton bones
        if nif_path is None:
            candidate = Path(hkx_path).parent / "skeleton.nif"
            if candidate.exists():
                nif_path = str(candidate)
        if nif_path and Path(nif_path).exists():
            self._skeleton_nif_path = nif_path
            self.skeleton.augment_from_nif(Path(nif_path))

        # Build classifier + session
        classifier = BoneClassifier()
        self.classifier_hidden = classifier.hidden_bones(self.skeleton.bone_names)
        self.pose_session = PoseSession(self.skeleton, classifier=classifier)

        # Hand the new pose_session to the viewport interact layer
        if self.viewport_interact is not None:
            self.viewport_interact.pose_session = self.pose_session
        else:
            from .viewport_interact import ViewportInteract
            self.viewport_interact = ViewportInteract(self.pose_session)

        # Playback writes into pose_session.playback_pose / _playback_active.
        # Switching skeletons drops any previously loaded frames — they were
        # sampled for the old skeleton's bone set and no longer apply.
        self.playback.clear()
        self.playback.pose_session = self.pose_session

        _log.info("Skeleton loaded: %d bones", self.skeleton.bone_count)

        self._frame_camera_on_skeleton()
        self._update_camera_bone_pos()

    def load_mesh(self, nif_path: str) -> None:
        from creation_lib.textures.texture_dirs import build_texture_dirs, create_ba2_manager
        from creation_lib.renderer.nif_loader import load_nif_to_scene
        from ui.editor.nif_session import NifSession

        if self.renderer is None:
            return
        program = self.renderer.programs.get("fo4")
        if program is None:
            return

        if "body" in self.registry.sessions:
            self.registry.remove_session("body")

        from app.paths import get_app_root
        texture_dirs, user_ba2, base_ba2 = build_texture_dirs(
            self.toolkit_settings,
            nif_path=nif_path,
            mods_root=get_app_root() / "mods",
        )
        if self.ba2_manager is None:
            self.ba2_manager = create_ba2_manager(user_ba2, base_ba2)

        scene_root, nif_file = load_nif_to_scene(
            nif_path, self.ctx, program, texture_dirs=texture_dirs,
            ba2_mgr=self.ba2_manager, nif_id="body",
        )
        session = NifSession(
            nif_id="body", nif=nif_file, file_path=nif_path,
            scene_root=scene_root, anim_manager=None,
        )
        self.registry.add_session(session)

        # Build skinned mesh
        self.skinned_meshes = []
        if self.skinned_renderer is not None:
            for block in nif_file.blocks:
                if nif_file.schema.is_subtype_of(block.type_name, "BSTriShape"):
                    skin_ref = block.get_field("Skin Instance") or block.get_field("Skin")
                    if isinstance(skin_ref, int) and skin_ref >= 0:
                        mesh = self.skinned_renderer.build_skinned_mesh(nif_file, block)
                        if mesh:
                            self.skinned_meshes.append(mesh)

        if self.skinned_meshes and self._skeleton_nif_path:
            from creation_lib.nif.rendering.skinned_renderer import attach_nif_bind_worlds
            attach_nif_bind_worlds(self._skeleton_nif_path, self.skinned_meshes)

    def load_composite_body(self, skeleton_hkx: str, skeleton_nif: str | None,
                            body_nif_paths: list[str], game: str) -> None:
        """Load skeleton + body parts, one SkinnedMesh per BSTriShape.

        Per-shape (not per-NIF) is load-bearing for Power Armor: within a
        single PA body/helmet NIF, different BSTriShape blocks can carry
        different BSSkin::BoneData inv_bind matrices for the same bone
        name, because each shape is authored in its own model-local
        origin. Any path that merges shapes and stores one inv_bind per
        bone name (first-occurrence wins) silently misplaces every later
        shape's vertices — e.g. pa_t60_Helmet has HelmetNeck:0 HEAD
        inv_bind t=(-137,12,0) [world-inverse] and PA_T60_Helmet:0 HEAD
        inv_bind t=(3.88,1.82,-0.11) [small mesh-local offset]; merging
        collapses the main helmet to the origin.
        """
        from creation_lib.nif import NifFile
        from creation_lib.nif.rendering.skinned_renderer import attach_nif_bind_worlds

        # Skeleton + PoseSession + viewport_interact wiring
        self.load_skeleton(skeleton_hkx, nif_path=skeleton_nif)

        if self.skinned_renderer is None:
            self.status_text = "Skinned renderer not initialized"
            return

        self.skinned_meshes = []
        loaded_nifs = 0
        for nif_path in body_nif_paths:
            try:
                nif_file = NifFile.load(nif_path)
            except Exception as e:
                _log.warning("Failed to load %s: %s", Path(nif_path).name, e)
                continue
            shape_count = 0
            for block in nif_file.blocks:
                if not nif_file.schema.is_subtype_of(block.type_name, "BSTriShape"):
                    continue
                skin_ref = block.get_field("Skin Instance") or block.get_field("Skin")
                if not isinstance(skin_ref, int) or skin_ref < 0:
                    continue
                mesh = self.skinned_renderer.build_skinned_mesh(nif_file, block)
                if mesh is None:
                    continue
                self.skinned_meshes.append(mesh)
                shape_count += 1
            if shape_count:
                loaded_nifs += 1

        if not self.skinned_meshes:
            self.status_text = "No body parts could be loaded"
            return

        if self._skeleton_nif_path:
            attach_nif_bind_worlds(self._skeleton_nif_path, self.skinned_meshes)

        self.status_text = (
            f"Body: {loaded_nifs} NIFs, {len(self.skinned_meshes)} shapes"
        )
        _log.info("Composite body loaded: %d NIFs, %d shapes",
                  loaded_nifs, len(self.skinned_meshes))

        self._frame_camera_on_skeleton()
        self._update_camera_bone_pos()

    def load_weapon(self, nif_path: str) -> None:
        from creation_lib.textures.texture_dirs import build_texture_dirs, create_ba2_manager
        from creation_lib.renderer.nif_loader import load_nif_to_scene
        from ui.editor.nif_session import NifSession

        if self.renderer is None:
            return
        program = self.renderer.programs.get("fo4")
        if program is None:
            return

        if "weapon" in self.registry.sessions:
            self.registry.remove_session("weapon")

        from app.paths import get_app_root
        texture_dirs, user_ba2, base_ba2 = build_texture_dirs(
            self.toolkit_settings,
            nif_path=nif_path,
            mods_root=get_app_root() / "mods",
        )
        if self.ba2_manager is None:
            self.ba2_manager = create_ba2_manager(user_ba2, base_ba2)

        scene_root, nif_file = load_nif_to_scene(
            nif_path, self.ctx, program, texture_dirs=texture_dirs,
            ba2_mgr=self.ba2_manager, nif_id="weapon",
        )
        session = NifSession(
            nif_id="weapon", nif=nif_file, file_path=nif_path,
            scene_root=scene_root, anim_manager=None,
        )
        self.registry.add_session(session)

        # SceneRenderer only draws self.scene_root — without this the weapon
        # NIF loads invisibly. The body mesh bypasses scene_root via the
        # skinned renderer in viewport_panel._render_skinned, so the two
        # render paths don't conflict.
        self.renderer.scene_root = scene_root
        self.renderer.clear_alt_vao_cache()
        self._apply_weapon_bone_transform(scene_root)

    def load_reference_pose(self, hkx_path: str) -> None:
        """Load a reference animation: frame 0 becomes the baseline pose,
        and the full clip is pre-sampled into the playback controller so
        the toolbar's Play / Loop buttons have something to play.
        """
        from ui.aligner.animation_loader import load_sighted_pose

        positions, rotations = load_sighted_pose(hkx_path)
        if self.pose_session is not None:
            self.pose_session.set_baseline(positions, rotations)
            self._update_camera_bone_pos()

        # Also try to load the full clip for playback. This is best-effort:
        # if extraction fails (unsupported compression, missing lib, etc.)
        # the baseline pose still works, only the Play/Loop buttons stay
        # disabled.
        try:
            self._load_playback_frames(hkx_path)
        except Exception as e:
            _log.warning("Playback frame sampling failed for %s: %s",
                         hkx_path, e)
            self.playback.clear()

    def _load_playback_frames(self, hkx_path: str) -> None:
        """Extract the full animation clip and pre-sample it into world-space
        per-frame poses for PlaybackController.
        """
        from creation_lib.havok.native_runtime import extract_clip_native
        from ui.bone_editor.playback_controller import sample_clip_to_world_frames
        from ui.aligner.animation_loader import unpack_hkx

        if self.skeleton is None:
            return

        skeleton_dict = {
            "bone_names": list(self.skeleton.bone_names),
            "parent_indices": list(self.skeleton.parent_indices),
            "ref_poses": [
                {
                    "t": self.skeleton.ref_translations[i].tolist(),
                    "q": self.skeleton.ref_rotations[i].tolist(),
                    "s": self.skeleton.ref_scales[i].tolist(),
                }
                for i in range(self.skeleton.bone_count)
            ],
        }

        tmp_xml = unpack_hkx(Path(hkx_path))
        try:
            xml_str = tmp_xml.read_text(encoding="utf-8")
            clip_data = extract_clip_native(xml_str)
        finally:
            tmp_xml.unlink(missing_ok=True)

        clip = _animation_clip_from_native_data(
            clip_data, bone_names=list(self.skeleton.bone_names)
        )
        if clip is None or not clip.channels:
            _log.info("No animation clip extracted from %s (empty / unsupported)",
                      hkx_path)
            self.playback.clear()
            return

        fps = _infer_clip_fps(clip, default=30.0)
        frames = sample_clip_to_world_frames(clip, skeleton_dict, fps=fps)
        if not frames:
            self.playback.clear()
            return
        self.playback.set_frames(frames, fps=fps)
        _log.info("Playback frames loaded: %d frames @ %.1ffps from %s",
                  len(frames), fps, Path(hkx_path).name)

    def _update_camera_bone_pos(self) -> None:
        """Sync ScopeCamera's first-person anchor to the Camera bone's world pos.

        SCOPE_VIEW reads `camera.camera_bone_pos`; without this the 1st-person
        view sits at the origin and the user sees nothing.
        """
        if self.pose_session is None:
            return
        world_pose = self.pose_session.get_world_pose()
        entry = world_pose.get("Camera")
        if entry is None:
            return
        import glm
        _, pos = entry
        self.camera.camera_bone_pos = glm.vec3(
            float(pos[0]), float(pos[1]), float(pos[2]),
        )

    def _apply_weapon_bone_transform(self, scene_root) -> None:
        """Position the weapon NIF at the Weapon bone's current world transform.

        Reads from `pose_session.get_world_pose()` so the weapon follows live
        pose edits (spine rotations, etc.). Falls back to RArm_Hand if there's
        no Weapon bone in the skeleton.
        """
        if scene_root is None or self.pose_session is None:
            return
        import glm
        from creation_lib.renderer.nif_loader import _update_world_transforms
        from creation_lib.bone_edit.quat_util import quat_to_matrix

        world_pose = self.pose_session.get_world_pose()
        entry = world_pose.get("Weapon") or world_pose.get("RArm_Hand")
        if entry is None:
            return
        rot_q, pos = entry
        rot_mat = quat_to_matrix(rot_q)

        mat = glm.mat4(1.0)
        for r in range(3):
            for c in range(3):
                mat[c][r] = float(rot_mat[r, c])
        mat[3][0] = float(pos[0])
        mat[3][1] = float(pos[1])
        mat[3][2] = float(pos[2])

        scene_root.transform = mat
        _update_world_transforms(scene_root, glm.mat4(1.0))

    def _frame_camera_on_skeleton(self) -> None:
        """Frame the orbit camera on the skeleton's world bounds."""
        if self.skeleton is None:
            return
        try:
            self.skeleton._ensure_world_transforms()
            wt = self.skeleton._world_translations
        except Exception:
            return
        if wt is None or len(wt) == 0:
            return
        import glm
        center = glm.vec3(
            float(wt[:, 0].mean()),
            float(wt[:, 1].mean()),
            float(wt[:, 2].mean()),
        )
        spread = float(max(wt.max(axis=0) - wt.min(axis=0)))
        self.camera.frame_on_bounds(center, spread * 0.5)

    def gui(self) -> None:
        """Per-frame entry point — only for floating panels and updates."""
        if self._first_frame:
            self.setup()
            self._init_panels()
            self._first_frame = False
