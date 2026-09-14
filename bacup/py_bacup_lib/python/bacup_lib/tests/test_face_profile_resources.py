from __future__ import annotations

import numpy as np

from bacup_lib.native_maps import native_face_resources_dir


PROFILE_VERTEX_COUNTS = {
    "male": 1696,
    "female": 1689,
    "ghoul_male": 1339,
    "ghoul_female": 1705,
    "child_male": 1339,
    "child_female": 1339,
}


def test_all_face_profiles_ship_compatible_correspondence_and_uv_lut() -> None:
    root = native_face_resources_dir()

    for profile, vertex_count in PROFILE_VERTEX_COUNTS.items():
        correspondence_path = root / f"fnv_to_fo4_correspondence_{profile}.npz"
        uv_lut_path = root / f"fnv_to_fo4_facetint_uv_lut_{profile}.npz"

        with np.load(correspondence_path) as correspondence:
            triangle_indices = correspondence["triangle_indices"]
            barycentrics = correspondence["barycentrics"]
            assert triangle_indices.shape == (vertex_count, 3)
            assert barycentrics.shape == (vertex_count, 3)
            assert triangle_indices.dtype == np.int32
            assert barycentrics.dtype == np.float32
            np.testing.assert_allclose(barycentrics.sum(axis=1), 1.0, atol=1e-5)

        with np.load(uv_lut_path) as uv_lut:
            uv = uv_lut["uv"]
            assert uv.shape == (1024, 1024, 2)
            assert uv.dtype == np.float32
            assert np.isfinite(uv[..., 0]).mean() > 0.7
