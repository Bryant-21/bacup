//! Apply the Pip-Boy 2000 worn-mesh fixes to an already converted Data tree, in place,
//! without a regen. The DATA_ROOT must hold `Meshes/Pipboy2000/CharacterAssets/`.
use std::path::PathBuf;

use conversion_native::phase::pipboy2000_nifs::fix_pipboy2000_nifs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: fix_pipboy2000_nifs DATA_ROOT")?;
    let summary = fix_pipboy2000_nifs(&data_root);
    println!(
        "rigid 1st-person shapes: {}, 3rd-person screen glow fixed: {}",
        summary.rigid_shapes, summary.screen_glow_fixed
    );
    if summary.failures.is_empty() {
        Ok(())
    } else {
        Err(summary.failures.join("; ").into())
    }
}
