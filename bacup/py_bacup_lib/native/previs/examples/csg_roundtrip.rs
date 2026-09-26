//! Dev harness: re-encodes retail `.csg` files and compares them byte for byte.
fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let layout = previs_native::csg::parse(&bytes).unwrap();
        let rebuilt = previs_native::csg::from_layout(&layout);
        let first_diff = bytes.iter().zip(&rebuilt).position(|(a, b)| a != b);
        println!(
            "{path}: rows={} stream={} original={} rebuilt={} identical={} first_diff={first_diff:?}",
            layout.rows.len(),
            layout.data.len(),
            bytes.len(),
            rebuilt.len(),
            bytes == rebuilt
        );
    }
}
