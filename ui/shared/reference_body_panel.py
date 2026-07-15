"""Reusable reference body preset selector widget.

Embeddable ImGui widget that shows a filterable list of pre-defined skeleton
profiles (game, skeleton type, gender, body parts) and calls a callback
when the user loads a selection. Used by bone_editor and weight_painter.
"""
from __future__ import annotations

import logging
from pathlib import Path
from typing import TYPE_CHECKING, Callable

from imgui_bundle import imgui

from creation_lib.skinning.reference_body import SKELETON_PROFILES

if TYPE_CHECKING:
    pass

_log = logging.getLogger("ui.shared.reference_body_panel")

# Callback signature:
#   on_load(skeleton_hkx: str, skeleton_nif: str | None,
#           body_nif_paths: list[str], game: str)
OnLoadCallback = Callable[[str, "str | None", "list[str]", str], None]


class ReferenceBodyPanel:
    """Filterable preset mesh selector for loading skeleton + body parts."""

    def __init__(self, toolkit_settings, on_load: OnLoadCallback,
                 games: list[str] | None = None):
        """
        Args:
            toolkit_settings: ToolkitSettings instance for path resolution.
            on_load: Callback when user loads a preset.
            games: Restrict to these game IDs (e.g. ["fo4"] for Havok-only
                tools). None = all games in SKELETON_PROFILES.
        """
        self._settings = toolkit_settings
        self._on_load = on_load

        # Build game list from profiles, optionally filtered
        if games is not None:
            self._game_ids = [g for g in games if g in SKELETON_PROFILES]
        else:
            self._game_ids = list(SKELETON_PROFILES.keys())
        self._game_idx = 0

        # Category, profile, variant, gender selection
        self._category_idx = 0
        self._profile_idx = 0
        self._variant_idx = 0
        self._gender_idx = 1  # 0=male, 1=female

        # Part checkboxes: part_key → checked
        self._part_flags: dict[str, bool] = {}

        # Status
        self._status = ""
        self._status_is_error = False

        # Initialize part flags for default selection
        self._refresh_parts()

    # ------------------------------------------------------------------ draw
    def draw(self):
        """Draw the preset selector UI. Call from within an imgui window."""
        # -- Game selector --
        game_labels = [gid.upper() for gid in self._game_ids]
        ch, self._game_idx = imgui.combo(
            "Game##ref_body", self._game_idx, game_labels,
        )
        if ch:
            self._category_idx = 0
            self._profile_idx = 0
            self._variant_idx = 0
            self._refresh_parts()

        game_id = self._game_ids[self._game_idx]
        profiles = SKELETON_PROFILES.get(game_id, {})
        if not profiles:
            imgui.text_colored(imgui.ImVec4(0.9, 0.4, 0.3, 1.0), "No profiles")
            return

        # -- Category selector --
        categories = self._get_categories(profiles)
        if self._category_idx >= len(categories):
            self._category_idx = 0
        ch, self._category_idx = imgui.combo(
            "Category##ref_body", self._category_idx, categories,
        )
        if ch:
            self._profile_idx = 0
            self._variant_idx = 0
            self._refresh_parts()

        category = categories[self._category_idx]
        profile_keys = self._get_profiles_for_category(profiles, category)
        if not profile_keys:
            imgui.text_colored(imgui.ImVec4(0.9, 0.4, 0.3, 1.0), "No profiles in category")
            return

        # -- Skeleton type selector (filtered by category) --
        profile_labels = [
            profiles[k].get("display_name", k.replace("_", " ").title())
            for k in profile_keys
        ]
        if self._profile_idx >= len(profile_keys):
            self._profile_idx = 0
        ch, self._profile_idx = imgui.combo(
            "Type##ref_body", self._profile_idx, profile_labels,
        )
        if ch:
            self._variant_idx = 0
            self._refresh_parts()

        profile_key = profile_keys[self._profile_idx]
        profile = profiles[profile_key]
        body_parts = profile.get("body_parts", {})
        variant_labels = self._get_variant_labels(profile)

        # -- Variant selector (e.g. PA suit, robot type) --
        if variant_labels:
            variant_keys = list(variant_labels.keys())
            variant_display = list(variant_labels.values())
            if self._variant_idx >= len(variant_keys):
                self._variant_idx = 0
            ch, self._variant_idx = imgui.combo(
                "Variant##ref_body", self._variant_idx, variant_display,
            )
            if ch:
                self._refresh_parts()
            current_variant = variant_keys[self._variant_idx]
        else:
            current_variant = None

        # -- Gender selector (only if profile has gendered parts) --
        has_gendered = any(
            k.startswith("male_") or k.startswith("female_")
            for k in body_parts
        )
        if has_gendered:
            ch, self._gender_idx = imgui.combo(
                "Gender##ref_body", self._gender_idx, ["Male", "Female"],
            )
            if ch:
                self._refresh_parts()

        # -- Body part checkboxes --
        visible_parts = self._get_visible_parts(
            body_parts, has_gendered, variant_labels, current_variant,
        )
        if visible_parts:
            imgui.text("Parts:")
            imgui.same_line()
            if imgui.small_button("All##ref_body"):
                for k in visible_parts:
                    self._part_flags[k] = True
            imgui.same_line()
            if imgui.small_button("None##ref_body"):
                for k in visible_parts:
                    self._part_flags[k] = False

            for part_key in visible_parts:
                display = self._strip_part_display(part_key, variant_labels)
                checked = self._part_flags.get(part_key, True)
                ch, checked = imgui.checkbox(f"{display}##ref_{part_key}", checked)
                if ch:
                    self._part_flags[part_key] = checked

        # -- Load button --
        imgui.spacing()
        any_selected = any(
            self._part_flags.get(k, True) for k in visible_parts
        )
        if not any_selected:
            imgui.begin_disabled()
        if imgui.button("Load Preset##ref_body", imgui.ImVec2(-1, 0)):
            self._do_load(game_id, profile_key, profile, visible_parts)
        if not any_selected:
            imgui.end_disabled()

        # -- Status --
        if self._status:
            color = (imgui.ImVec4(0.9, 0.4, 0.3, 1.0) if self._status_is_error
                     else imgui.ImVec4(0.4, 0.9, 0.4, 1.0))
            imgui.text_colored(color, self._status)

    # ----------------------------------------------------------- internals
    def _get_categories(self, profiles: dict) -> list[str]:
        """Return ordered unique categories from the profile dict."""
        seen: list[str] = []
        for p in profiles.values():
            cat = p.get("category", "Other")
            if cat not in seen:
                seen.append(cat)
        return seen

    def _get_profiles_for_category(self, profiles: dict, category: str) -> list[str]:
        """Return profile keys whose category matches."""
        return [k for k, p in profiles.items()
                if p.get("category", "Other") == category]

    def _get_variant_labels(self, profile: dict) -> dict[str, str]:
        """Return variant_labels dict from profile, or empty dict."""
        return profile.get("variant_labels", {})

    def _get_visible_parts(
        self,
        body_parts: dict,
        has_gendered: bool,
        variant_labels: dict[str, str],
        current_variant: str | None,
    ) -> list[str]:
        """Return part keys visible for current variant + gender selection."""
        all_variant_keys = set(variant_labels.keys())
        gender_prefix = "male_" if self._gender_idx == 0 else "female_"
        visible = []
        for k in body_parts:
            # Determine if this key belongs to a specific variant
            part_variant = next(
                (vk for vk in all_variant_keys if k.startswith(vk + "_")),
                None,
            )
            if part_variant is not None:
                # Only show the currently selected variant
                if part_variant == current_variant:
                    visible.append(k)
            elif k.startswith("male_") or k.startswith("female_"):
                # Gender-filtered part
                if has_gendered and k.startswith(gender_prefix):
                    visible.append(k)
            else:
                # Unprefixed part — always visible (e.g. PA frame)
                visible.append(k)
        return visible

    def _strip_part_display(self, part_key: str, variant_labels: dict[str, str]) -> str:
        """Strip variant and gender prefixes, title-case for display."""
        display = part_key
        for vk in variant_labels:
            if display.startswith(vk + "_"):
                display = display[len(vk) + 1:]
                break
        for prefix in ("male_", "female_"):
            if display.startswith(prefix):
                display = display[len(prefix):]
                break
        return display.replace("_", " ").title()

    def _refresh_parts(self):
        """Rebuild part flags when game/category/profile/variant/gender changes."""
        game_id = self._game_ids[self._game_idx]
        profiles = SKELETON_PROFILES.get(game_id, {})
        categories = self._get_categories(profiles)
        if not categories:
            self._part_flags = {}
            return
        if self._category_idx >= len(categories):
            self._category_idx = 0
        category = categories[self._category_idx]
        profile_keys = self._get_profiles_for_category(profiles, category)
        if not profile_keys:
            self._part_flags = {}
            return
        if self._profile_idx >= len(profile_keys):
            self._profile_idx = 0
        profile = profiles[profile_keys[self._profile_idx]]
        body_parts = profile.get("body_parts", {})
        variant_labels = self._get_variant_labels(profile)
        if variant_labels:
            variant_keys = list(variant_labels.keys())
            if self._variant_idx >= len(variant_keys):
                self._variant_idx = 0
            current_variant = variant_keys[self._variant_idx]
        else:
            current_variant = None
        has_gendered = any(
            k.startswith("male_") or k.startswith("female_") for k in body_parts
        )
        visible = self._get_visible_parts(
            body_parts, has_gendered, variant_labels, current_variant,
        )
        self._part_flags = {k: True for k in visible}
        self._status = ""

    def _do_load(self, game_id: str, profile_key: str, profile: dict,
                 visible_parts: list[str]):
        """Resolve paths and call on_load callback."""
        game_paths = self._settings.get_game_paths(game_id)
        extracted_dir = game_paths.get("extracted_dir", "")
        if not extracted_dir or not Path(extracted_dir).is_dir():
            self._status = f"No extracted_dir for {game_id.upper()} — configure in Settings > Paths"
            self._status_is_error = True
            return

        extracted = Path(extracted_dir)
        body_parts = profile.get("body_parts", {})

        # Resolve skeleton paths
        skeleton_hkx_rel = profile.get("skeleton_hkx", "")
        skeleton_hkx = str(extracted / skeleton_hkx_rel) if skeleton_hkx_rel else ""

        skeleton_nif_rel = profile.get("skeleton_nif", "")
        skeleton_nif = str(extracted / skeleton_nif_rel) if skeleton_nif_rel else None

        # Resolve selected body part paths
        selected = [
            k for k in visible_parts
            if self._part_flags.get(k, True) and k in body_parts
        ]
        body_nif_paths = []
        missing = []
        for part_key in selected:
            rel = body_parts[part_key]
            full = extracted / rel
            if full.exists():
                body_nif_paths.append(str(full))
            else:
                missing.append(part_key)

        if missing:
            _log.warning("Missing body part NIFs: %s", missing)

        if not body_nif_paths:
            self._status = f"No body part NIFs found (missing: {', '.join(missing)})"
            self._status_is_error = True
            return

        if skeleton_hkx and not Path(skeleton_hkx).exists():
            self._status = f"Skeleton HKX not found: {skeleton_hkx_rel}"
            self._status_is_error = True
            return

        _log.info("Loading preset %s/%s: %d parts, skeleton=%s",
                  game_id, profile_key, len(body_nif_paths), skeleton_hkx_rel)

        try:
            self._on_load(skeleton_hkx, skeleton_nif, body_nif_paths, game_id)
            variant_labels = self._get_variant_labels(profile)
            parts_str = ", ".join(
                self._strip_part_display(k, variant_labels)
                for k in selected if k in body_parts
            )
            self._status = f"Loaded: {parts_str}"
            if missing:
                self._status += f" (missing: {', '.join(missing)})"
            self._status_is_error = bool(missing)
        except Exception as e:
            _log.exception("Failed to load preset")
            self._status = f"Error: {e}"
            self._status_is_error = True
