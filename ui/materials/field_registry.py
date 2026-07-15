"""Declarative field definitions for BGSM/BGEM material editors.

Every editable field is described by a FieldDef. Three module-level lists
(GENERAL_FIELDS, MATERIAL_FIELDS, EFFECT_FIELDS) drive the entire UI:
panels iterate the list, call draw_field() for each entry, done.

Version visibility rules and dependency rules come from the C# reference
editor (refs/material-editor/Main.cs lines 710-1105).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable


@dataclass
class FieldDef:
    name: str           # Display label ("Specular Color")
    attr: str           # Python attribute on BGSMData/BGEMData/BaseHeader
    kind: str           # "bool" | "float" | "int" | "color3" | "string" | "dropdown"
    source: str         # "header" | "bgsm" | "bgem"
    version_visible: Callable[[int], bool] | None = None  # None = always visible
    depends_on: str | None = None       # parent field attr that must be truthy
    tooltip: str = ""
    dropdown_items: list[str] | None = None


# ---------------------------------------------------------------------------
# Version visibility helpers
# ---------------------------------------------------------------------------
def _v_lt(n: int) -> Callable[[int], bool]:
    return lambda v: v < n

def _v_le(n: int) -> Callable[[int], bool]:
    return lambda v: v <= n

def _v_gt(n: int) -> Callable[[int], bool]:
    return lambda v: v > n

def _v_ge(n: int) -> Callable[[int], bool]:
    return lambda v: v >= n

def _v_eq(n: int) -> Callable[[int], bool]:
    return lambda v: v == n


_FO4_BGSM_VERSION = 2
_FO76_BGSM_VERSION = 20


# ===================================================================
# GENERAL FIELDS  (BaseHeader — shared by BGSM and BGEM)
# ===================================================================
GENERAL_FIELDS: list[FieldDef] = [
    FieldDef("Tile U",                  "tile_u",                       "bool",     "header"),
    FieldDef("Tile V",                  "tile_v",                       "bool",     "header"),
    FieldDef("Offset U",               "u_offset",                     "float",    "header"),
    FieldDef("Offset V",               "v_offset",                     "float",    "header"),
    FieldDef("Scale U",                "u_scale",                      "float",    "header"),
    FieldDef("Scale V",                "v_scale",                      "float",    "header"),
    FieldDef("Alpha",                  "alpha",                        "float",    "header"),
    FieldDef("Alpha Blend Mode",       "alpha_blend_mode0",            "dropdown", "header",
             dropdown_items=["Unknown", "None", "Standard", "Additive", "Multiplicative"]),
    FieldDef("Alpha Test Reference",   "alpha_test_ref",               "int",      "header"),
    FieldDef("Alpha Test",             "alpha_test",                   "bool",     "header"),
    FieldDef("Z Buffer Write",         "zbuffer_write",                "bool",     "header"),
    FieldDef("Z Buffer Test",          "zbuffer_test",                 "bool",     "header"),
    FieldDef("Screen Space Reflections","ssr",                         "bool",     "header"),
    FieldDef("Wetness Control SSR",    "wet_ssr",                      "bool",     "header"),
    FieldDef("Decal",                  "decal",                        "bool",     "header"),
    FieldDef("Two Sided",              "two_sided",                    "bool",     "header"),
    FieldDef("Decal No Fade",          "decal_nofade",                 "bool",     "header"),
    FieldDef("Non Occluder",           "non_occluder",                 "bool",     "header"),
    FieldDef("Refraction",             "refraction",                   "bool",     "header"),
    FieldDef("Refraction Falloff",     "refraction_falloff",           "bool",     "header",
             depends_on="refraction"),
    FieldDef("Refraction Power",       "refraction_power",             "float",    "header",
             depends_on="refraction"),
    FieldDef("Environment Mapping",    "env_mapping",                  "bool",     "header",
             version_visible=_v_lt(10)),
    FieldDef("Environment Mask Scale", "env_mapping_mask_scale",       "float",    "header",
             version_visible=_v_lt(10), depends_on="env_mapping"),
    FieldDef("Depth Bias",             "depth_bias",                   "bool",     "header",
             version_visible=_v_ge(10)),
    FieldDef("Grayscale To Palette Color", "grayscale_to_palette_color","bool",    "header"),
    FieldDef("Mask Writes",            "mask_writes",                  "int",      "header",
             version_visible=_v_ge(6)),
]


# ===================================================================
# MATERIAL FIELDS  (BGSMData — only shown for .bgsm files)
# ===================================================================
MATERIAL_FIELDS: list[FieldDef] = [
    # --- Textures ---
    FieldDef("Diffuse",                "DiffuseTexture",               "texture_path", "bgsm"),
    FieldDef("Normal",                 "NormalTexture",                "texture_path", "bgsm"),
    FieldDef("Smooth Spec",            "SmoothSpecTexture",            "texture_path", "bgsm"),
    FieldDef("Greyscale",              "GreyscaleTexture",             "texture_path", "bgsm"),
    FieldDef("Environment",            "EnvmapTexture",                "texture_path", "bgsm",
             version_visible=_v_le(2)),
    FieldDef("Glow",                   "GlowTexture",                  "texture_path", "bgsm"),
    FieldDef("Inner Layer",            "InnerLayerTexture",            "texture_path", "bgsm",
             version_visible=_v_le(2)),
    FieldDef("Wrinkles",               "WrinklesTexture",              "texture_path", "bgsm"),
    FieldDef("Displacement",           "DisplacementTexture",          "texture_path", "bgsm",
             version_visible=_v_le(2)),
    FieldDef("Specular",               "SpecularTexture",              "texture_path", "bgsm",
             version_visible=_v_gt(2)),
    FieldDef("Lighting",               "LightingTexture",              "texture_path", "bgsm",
             version_visible=_v_gt(2)),
    FieldDef("Flow",                   "FlowTexture",                  "texture_path", "bgsm",
             version_visible=_v_gt(2)),
    FieldDef("Distance Field Alpha",   "DistanceFieldAlphaTexture",    "texture_path", "bgsm",
             version_visible=_v_gt(2)),

    # --- Material properties ---
    FieldDef("Enable Editor Alpha Ref","EnableEditorAlphaRef",         "bool",     "bgsm"),

    # v < 8 block
    FieldDef("Rim Lighting",           "RimLighting",                  "bool",     "bgsm",
             version_visible=_v_lt(8)),
    FieldDef("Rim Power",              "RimPower",                     "float",    "bgsm",
             version_visible=_v_lt(8), depends_on="RimLighting"),
    FieldDef("Backlight Power",        "BackLightPower",               "float",    "bgsm",
             version_visible=_v_lt(8)),
    FieldDef("Subsurface Lighting",    "SubsurfaceLighting",           "bool",     "bgsm",
             version_visible=_v_lt(8)),
    FieldDef("Subsurface Lighting Rolloff", "SubsurfaceLightingRolloff","float",   "bgsm",
             version_visible=_v_lt(8), depends_on="SubsurfaceLighting"),

    # v >= 8 block (translucency)
    FieldDef("Translucency",           "Translucency",                 "bool",     "bgsm",
             version_visible=_v_ge(8)),
    FieldDef("Transl. Thick Object",   "TranslucencyThickObject",      "bool",     "bgsm",
             version_visible=_v_ge(8)),
    FieldDef("Transl. Alb+Subsurf Color","TranslucencyMixAlbedoWithSubsurfaceColor","bool","bgsm",
             version_visible=_v_ge(8)),
    FieldDef("Transl. Subsurface Color","TranslucencySubsurfaceColor", "color3",   "bgsm",
             version_visible=_v_ge(8)),
    FieldDef("Transl. Transmissive Scale","TranslucencyTransmissiveScale","float", "bgsm",
             version_visible=_v_ge(8)),
    FieldDef("Transl. Turbulence",     "TranslucencyTurbulence",       "float",    "bgsm",
             version_visible=_v_ge(8)),

    # Specular
    FieldDef("Specular Enabled",       "SpecularEnabled",              "bool",     "bgsm"),
    FieldDef("Specular Color",         "SpecularColor",                "color3",   "bgsm",
             depends_on="SpecularEnabled"),
    FieldDef("Specular Multiplier",    "SpecularMult",                 "float",    "bgsm",
             depends_on="SpecularEnabled"),
    FieldDef("Smoothness",             "Smoothness",                   "float",    "bgsm"),
    FieldDef("Fresnel Power",          "FresnelPower",                 "float",    "bgsm"),

    # Wetness
    FieldDef("Wet Spec Scale",         "WetnessControlSpecScale",      "float",    "bgsm"),
    FieldDef("Wet Spec Power Scale",   "WetnessControlSpecPowerScale", "float",    "bgsm"),
    FieldDef("Wet Spec Min Var",       "WetnessControlSpecMinvar",     "float",    "bgsm"),
    FieldDef("Wet Env Map Scale",      "WetnessControlEnvMapScale",    "float",    "bgsm",
             version_visible=_v_lt(10)),
    FieldDef("Wet Fresnel Power",      "WetnessControlFresnelPower",   "float",    "bgsm"),
    FieldDef("Wet Metalness",          "WetnessControlMetalness",      "float",    "bgsm"),

    # Fallout 76-specific surface fields
    FieldDef("PBR",                    "PBR",                          "bool",     "bgsm",
             version_visible=_v_ge(_FO76_BGSM_VERSION)),
    FieldDef("Custom Porosity",        "CustomPorosity",               "bool",     "bgsm",
             version_visible=_v_ge(_FO76_BGSM_VERSION)),
    FieldDef("Porosity Value",         "PorosityValue",                "float",    "bgsm",
             version_visible=_v_ge(_FO76_BGSM_VERSION)),

    FieldDef("Root Material Path",     "RootMaterialPath",             "material_path", "bgsm"),
    FieldDef("Aniso Lighting",         "AnisoLighting",                "bool",     "bgsm"),

    # Emittance
    FieldDef("Emittance Enabled",      "EmitEnabled",                  "bool",     "bgsm"),
    FieldDef("Emittance Color",        "EmittanceColor",               "color3",   "bgsm",
             depends_on="EmitEnabled"),
    FieldDef("Emittance Multiplier",   "EmittanceMult",                "float",    "bgsm",
             depends_on="EmitEnabled"),
    FieldDef("Model Space Normals",    "ModelSpaceNormals",            "bool",     "bgsm"),
    FieldDef("External Emittance",     "ExternalEmittance",            "bool",     "bgsm"),
    FieldDef("Lum Emittance",          "LumEmittance",                 "float",    "bgsm",
             version_visible=_v_ge(12)),

    # Adaptative emissive
    FieldDef("Adaptative Emissive",    "UseAdaptativeEmissive",        "bool",     "bgsm",
             version_visible=_v_ge(13)),
    FieldDef("Adapt. Em. Exposure Offset",    "AdaptativeEmissive_ExposureOffset",    "float", "bgsm",
             version_visible=_v_ge(13), depends_on="UseAdaptativeEmissive"),
    FieldDef("Adapt. Em. Final Exposure Min", "AdaptativeEmissive_FinalExposureMin",  "float", "bgsm",
             version_visible=_v_ge(13), depends_on="UseAdaptativeEmissive"),
    FieldDef("Adapt. Em. Final Exposure Max", "AdaptativeEmissive_FinalExposureMax",  "float", "bgsm",
             version_visible=_v_ge(13), depends_on="UseAdaptativeEmissive"),

    # Misc flags
    FieldDef("Back Lighting",          "BackLighting",                 "bool",     "bgsm",
             version_visible=_v_lt(8)),
    FieldDef("Receive Shadows",        "ReceiveShadows",               "bool",     "bgsm"),
    FieldDef("Hide Secret",            "HideSecret",                   "bool",     "bgsm"),
    FieldDef("Cast Shadows",           "CastShadows",                  "bool",     "bgsm"),
    FieldDef("Dissolve Fade",          "DissolveFade",                 "bool",     "bgsm"),
    FieldDef("Assume Shadowmask",      "AssumeShadowmask",             "bool",     "bgsm"),
    FieldDef("Glowmap",                "Glowmap",                      "bool",     "bgsm"),
    FieldDef("Environment Map Window", "EnvironmentMappingWindow",     "bool",     "bgsm",
             version_visible=_v_lt(7)),
    FieldDef("Environment Map Eye",    "EnvironmentMappingEye",        "bool",     "bgsm",
             version_visible=_v_lt(7)),

    # Hair
    FieldDef("Hair",                   "Hair",                         "bool",     "bgsm"),
    FieldDef("Hair Tint Color",        "HairTintColor",                "color3",   "bgsm",
             depends_on="Hair"),

    FieldDef("Tree",                   "Tree",                         "bool",     "bgsm"),
    FieldDef("Facegen",                "Facegen",                      "bool",     "bgsm"),
    FieldDef("Skin Tint",              "SkinTint",                     "bool",     "bgsm"),

    # Tessellation
    FieldDef("Tessellate",             "Tessellate",                   "bool",     "bgsm"),
    FieldDef("Displacement Tex Bias",  "DisplacementTextureBias",      "float",    "bgsm",
             version_visible=_v_lt(3), depends_on="Tessellate"),
    FieldDef("Displacement Tex Scale", "DisplacementTextureScale",     "float",    "bgsm",
             version_visible=_v_lt(3), depends_on="Tessellate"),
    FieldDef("Tessellation PN Scale",  "TessellationPnScale",          "float",    "bgsm",
             version_visible=_v_lt(3), depends_on="Tessellate"),
    FieldDef("Tessellation Base Factor","TessellationBaseFactor",      "float",    "bgsm",
             version_visible=_v_lt(3), depends_on="Tessellate"),
    FieldDef("Tessellation Fade Distance","TessellationFadeDistance",  "float",    "bgsm",
             version_visible=_v_lt(3), depends_on="Tessellate"),

    FieldDef("Grayscale To Palette Scale","GrayscaleToPaletteScale",   "float",    "bgsm"),
    FieldDef("Skew Specular Alpha",    "SkewSpecularAlpha",            "bool",     "bgsm",
             version_visible=_v_ge(1)),

    # Terrain
    FieldDef("Terrain",                "Terrain",                      "bool",     "bgsm",
             version_visible=_v_ge(3)),
    FieldDef("Unk Int 1 BGSM",        "UnkInt1",                      "int",      "bgsm",
             version_visible=_v_eq(3), depends_on="Terrain"),
    FieldDef("Terrain Threshold Falloff","TerrainThresholdFalloff",    "float",    "bgsm",
             version_visible=_v_ge(3), depends_on="Terrain"),
    FieldDef("Terrain Tiling Distance","TerrainTilingDistance",        "float",    "bgsm",
             version_visible=_v_ge(3), depends_on="Terrain"),
    FieldDef("Terrain Rotation Angle", "TerrainRotationAngle",         "float",    "bgsm",
             version_visible=_v_ge(3), depends_on="Terrain"),
]


# ===================================================================
# EFFECT FIELDS  (BGEMData — only shown for .bgem files)
# ===================================================================
EFFECT_FIELDS: list[FieldDef] = [
    # --- Textures ---
    FieldDef("Base Texture",           "BaseTexture",                  "texture_path", "bgem"),
    FieldDef("Grayscale Texture",      "GrayscaleTexture",             "texture_path", "bgem"),
    FieldDef("Envmap Texture",         "EnvmapTexture",                "texture_path", "bgem"),
    FieldDef("Normal Texture",         "NormalTexture",                "texture_path", "bgem"),
    FieldDef("Envmap Mask Texture",    "EnvmapMaskTexture",            "texture_path", "bgem"),
    FieldDef("Specular Texture",       "SpecularTexture",              "texture_path", "bgem",
             version_visible=_v_ge(11)),
    FieldDef("Lighting Texture",       "LightingTexture",              "texture_path", "bgem",
             version_visible=_v_ge(11)),
    FieldDef("Glow Texture",           "GlowTexture",                  "texture_path", "bgem",
             version_visible=_v_ge(11)),

    # Glass (v >= 21)
    FieldDef("Glass Roughness Scratch","GlassRoughnessScratch",        "texture_path", "bgem",
             version_visible=_v_ge(21)),
    FieldDef("Glass Dirt Overlay",     "GlassDirtOverlay",             "texture_path", "bgem",
             version_visible=_v_ge(21)),
    FieldDef("Glass Enabled",          "GlassEnabled",                 "bool",     "bgem",
             version_visible=_v_ge(21)),
    FieldDef("Glass Fresnel Color",    "GlassFresnelColor",            "color3",   "bgem",
             version_visible=_v_ge(21), depends_on="GlassEnabled"),
    FieldDef("Glass Blur Scale Base",  "GlassBlurScaleBase",           "float",    "bgem",
             version_visible=_v_ge(21), depends_on="GlassEnabled"),
    FieldDef("Glass Blur Scale Factor","GlassBlurScaleFactor",         "float",    "bgem",
             version_visible=_v_ge(22), depends_on="GlassEnabled"),
    FieldDef("Glass Refraction Scale Base","GlassRefractionScaleBase", "float",    "bgem",
             version_visible=_v_ge(21), depends_on="GlassEnabled"),

    # Environment mapping (v >= 10)
    FieldDef("Env Mapping",            "EnvironmentMapping",           "bool",     "bgem",
             version_visible=_v_ge(10)),
    FieldDef("Env Mapping Mask Scale", "EnvironmentMappingMaskScale",  "float",    "bgem",
             version_visible=_v_ge(10)),

    # Flags
    FieldDef("Blood Enabled",          "BloodEnabled",                 "bool",     "bgem"),
    FieldDef("Effect Lighting Enabled","EffectLightingEnabled",        "bool",     "bgem"),
    FieldDef("Falloff Enabled",        "FalloffEnabled",               "bool",     "bgem"),
    FieldDef("Falloff Color Enabled",  "FalloffColorEnabled",          "bool",     "bgem"),
    FieldDef("Grayscale To Palette Alpha","GrayscaleToPaletteAlpha",   "bool",     "bgem"),
    FieldDef("Soft Enabled",           "SoftEnabled",                  "bool",     "bgem"),

    # Colors / scales
    FieldDef("Base Color",             "BaseColor",                    "color3",   "bgem"),
    FieldDef("Base Color Scale",       "BaseColorScale",               "float",    "bgem"),

    # Falloff parameters
    FieldDef("Falloff Start Angle",    "FalloffStartAngle",            "float",    "bgem",
             depends_on="FalloffEnabled"),
    FieldDef("Falloff Stop Angle",     "FalloffStopAngle",             "float",    "bgem",
             depends_on="FalloffEnabled"),
    FieldDef("Falloff Start Opacity",  "FalloffStartOpacity",          "float",    "bgem",
             depends_on="FalloffEnabled"),
    FieldDef("Falloff Stop Opacity",   "FalloffStopOpacity",           "float",    "bgem",
             depends_on="FalloffEnabled"),

    FieldDef("Lighting Influence",     "LightingInfluence",            "float",    "bgem"),
    FieldDef("Envmap Min LOD",         "EnvmapMinLOD",                 "int",      "bgem"),
    FieldDef("Soft Depth",             "SoftDepth",                    "float",    "bgem",
             depends_on="SoftEnabled"),

    # Emittance (effect)
    FieldDef("Emit Color",             "EmittanceColor",               "color3",   "bgem",
             version_visible=_v_ge(11)),
    FieldDef("Adaptative Em. Exposure Offset",    "AdaptativeEmissive_ExposureOffset",    "float", "bgem",
             version_visible=_v_ge(15)),
    FieldDef("Adaptative Em. Final Exp. Min",     "AdaptativeEmissive_FinalExposureMin",  "float", "bgem",
             version_visible=_v_ge(15)),
    FieldDef("Adaptative Em. Final Exp. Max",     "AdaptativeEmissive_FinalExposureMax",  "float", "bgem",
             version_visible=_v_ge(15)),
    FieldDef("Effect Glowmap",         "Glowmap",                      "bool",     "bgem",
             version_visible=_v_ge(16)),
    FieldDef("Effect PBR Specular",    "EffectPbrSpecular",            "bool",     "bgem",
             version_visible=_v_ge(20)),
]


# ===================================================================
# SHADER FIELDS  (BSLightingShaderProperty — NIF-level, BGSM only)
# Bits / labels come from nif.xml `Fallout4ShaderPropertyFlags1/2` and the
# `BSLightingShaderType` enum. Stored on the Max material as integer
# (dropdown selection, 1-based) and per-bit bools.
# ===================================================================
BSLIGHTING_SHADER_TYPES: list[str] = [
    "Default",
    "Environment Map",
    "Glow Shader",
    "Parallax",
    "Facegen",
    "Skin Tint",
    "Hair Tint",
    "Parallax Occ",
    "Multitexture Landscape",
    "LOD Landscape",
    "Snow",
    "MultiLayer Parallax",
    "Tree Anim",
    "LOD Objects",
    "Sparkle Snow",
    "LOD Objects HD",
    "Eye Envmap",
    "Cloud",
    "LOD Landscape Noise",
    "Multitex Landscape LOD Blend",
    "FO4 Dismemberment",
]

# (display name, bit). attr is derived as nif_sf1_<lowercased_name>.
F4_SHADER_FLAGS_1: list[tuple[str, int]] = [
    ("Specular", 0), ("Skinned", 1), ("Temp_Refraction", 2), ("Vertex_Alpha", 3),
    ("GS_To_Palette_Color", 4), ("GS_To_Palette_Alpha", 5), ("Use_Falloff", 6),
    ("Environment_Mapping", 7), ("RGB_Falloff", 8), ("Cast_Shadows", 9),
    ("Face", 10), ("UI_Mask_Rects", 11), ("Model_Space_Normals", 12),
    ("Non_Projective_Shadows", 13), ("Landscape", 14), ("Refraction", 15),
    ("Fire_Refraction", 16), ("Eye_Env_Mapping", 17), ("Hair", 18),
    ("Screendoor_Alpha_Fade", 19), ("Localmap_Hide_Secret", 20), ("Skin_Tint", 21),
    ("Own_Emit", 22), ("Projected_UV", 23), ("Multiple_Textures", 24),
    ("Tessellate", 25), ("Decal", 26), ("Dynamic_Decal", 27),
    ("Character_Lighting", 28), ("External_Emittance", 29), ("Soft_Effect", 30),
    ("ZBuffer_Test", 31),
]

F4_SHADER_FLAGS_2: list[tuple[str, int]] = [
    ("ZBuffer_Write", 0), ("LOD_Landscape", 1), ("LOD_Objects", 2), ("No_Fade", 3),
    ("Double_Sided", 4), ("Vertex_Colors", 5), ("Glow_Map", 6), ("Transform_Changed", 7),
    ("Dismemberment_Meatcuff", 8), ("Tint", 9), ("Grass_Vertex_Lighting", 10),
    ("Grass_Uniform_Scale", 11), ("Grass_Fit_Slope", 12), ("Grass_Billboard", 13),
    ("No_LOD_Land_Blend", 14), ("Dismemberment", 15), ("Wireframe", 16),
    ("Weapon_Blood", 17), ("Hide_On_Local_Map", 18), ("Premult_Alpha", 19),
    ("VATS_Target", 20), ("Anisotropic_Lighting", 21), ("Skew_Specular_Alpha", 22),
    ("Menu_Screen", 23), ("Multi_Layer_Parallax", 24), ("Alpha_Test", 25),
    ("Gradient_Remap", 26), ("VATS_Target_Draw_All", 27), ("Pipboy_Screen", 28),
    ("Tree_Anim", 29), ("Effect_Lighting", 30), ("Refraction_Writes_Depth", 31),
]


def _sf1_attr(name: str) -> str:
    return f"nif_sf1_{name.lower()}"


def _sf2_attr(name: str) -> str:
    return f"nif_sf2_{name.lower()}"


SHADER_FLAGS_1_BITS: dict[str, int] = {_sf1_attr(name): bit for name, bit in F4_SHADER_FLAGS_1}
SHADER_FLAGS_2_BITS: dict[str, int] = {_sf2_attr(name): bit for name, bit in F4_SHADER_FLAGS_2}


SHADER_FIELDS: list[FieldDef] = [
    FieldDef("Shader Type", "nif_shader_type", "dropdown", "shader_nif",
             dropdown_items=BSLIGHTING_SHADER_TYPES),
]
SHADER_FIELDS.extend(
    FieldDef(name.replace("_", " "), _sf1_attr(name), "bool", "shader_nif")
    for name, _bit in F4_SHADER_FLAGS_1
)
SHADER_FIELDS.extend(
    FieldDef(name.replace("_", " "), _sf2_attr(name), "bool", "shader_nif")
    for name, _bit in F4_SHADER_FLAGS_2
)


# ---------------------------------------------------------------------------
# Default values by kind (used when flattening None fields)
# ---------------------------------------------------------------------------
KIND_DEFAULTS: dict[str, object] = {
    "bool": False,
    "float": 0.0,
    "int": 0,
    "color3": (1.0, 1.0, 1.0),
    "string": "",
    "dropdown": 0,
    "texture_path": "",
    "material_path": "",
}

# Build attr → FieldDef lookup for quick access
_ALL_FIELDS = GENERAL_FIELDS + MATERIAL_FIELDS + EFFECT_FIELDS + SHADER_FIELDS
FIELD_BY_ATTR: dict[str, FieldDef] = {f.attr: f for f in _ALL_FIELDS}
