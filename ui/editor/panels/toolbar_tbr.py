"""Lighting tuning presets for the NIF editor shader."""

LIGHTING_PRESETS = {
    "Standard": {
        "_dbg_envBoost": 1.0,
        "_dbg_metalF0": 0.9,
        "_dbg_diffuseBleed": 0.0,
        "_dbg_exposure": 4.23,
        "_dbg_specBoost": 1.0,
        "_dbg_ambientBoost": 1.0,
    },
    "Fallout 4": {
        "_dbg_envBoost": 2.0,
        "_dbg_metalF0": 0.9,
        "_dbg_diffuseBleed": 0.0,
        "_dbg_exposure": 4.2,
        "_dbg_specBoost": 1.0,
        "_dbg_ambientBoost": 1.0,
    },
    "Fallout 76": {
        "_dbg_envBoost": 0.3,
        "_dbg_metalF0": 0.5,
        "_dbg_diffuseBleed": 1.0,
        "_dbg_exposure": 5.5,
        "_dbg_specBoost": 7.5,
        "_dbg_ambientBoost": 3.5,
    },
    "Skyrim SE": {
        "_dbg_envBoost": 1.2,
        "_dbg_metalF0": 0.7,
        "_dbg_diffuseBleed": 0.1,
        "_dbg_exposure": 4.5,
        "_dbg_specBoost": 0.8,
        "_dbg_ambientBoost": 1.2,
    },
    "Gamebryo": {
        "_dbg_envBoost": 1.2,
        "_dbg_metalF0": 0.7,
        "_dbg_diffuseBleed": 0.1,
        "_dbg_exposure": 4.5,
        "_dbg_specBoost": 0.8,
        "_dbg_ambientBoost": 1.2,
    },
    "Starfield": {
        # Calibrated for the SF parallel render engine (ui/editor/sf_engine.py),
        # which inherits the standalone tools/sf_render_test.py baseline
        # (uBrightnessScale=0.1, uEnvIntensity=8.0). Reaching FO4 visual parity
        # in that pipeline requires aggressive boosts on env / spec / exposure.
        "_dbg_envBoost": 5.0,
        "_dbg_metalF0": 0.5,
        "_dbg_diffuseBleed": 0.4,
        "_dbg_exposure": 12.0,
        "_dbg_specBoost": 5.0,
        "_dbg_ambientBoost": 4.2,
    },
    "Custom": None,  # placeholder — keeps current slider values
}

# Maps GameProfile.id → lighting preset name for auto-switching on NIF load
GAME_ID_TO_PRESET = {
    "fo4": "Fallout 4",
    "fo76": "Fallout 76",
    "oblivion": "Gamebryo",
    "fo3": "Gamebryo",
    "fnv": "Gamebryo",
    "skyrimse": "Skyrim SE",
    "starfield": "Starfield",
}

LIGHTING_PRESET_NAMES = list(LIGHTING_PRESETS.keys())

# Backward compat aliases
TBR_PRESETS = LIGHTING_PRESETS
TBR_PRESET_NAMES = LIGHTING_PRESET_NAMES


def apply_lighting_preset(app, name: str):
    """Apply a lighting preset to the renderer's debug uniforms."""
    preset = LIGHTING_PRESETS.get(name)
    if preset is None:
        return  # "Custom" — don't change anything
    target = getattr(app, 'renderer', app)
    for attr, val in preset.items():
        setattr(target, attr, val)


# Backward compat alias
apply_tbr_preset = apply_lighting_preset
