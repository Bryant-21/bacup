pub type Point = [f64; 3];
pub type Triangle = [Point; 3];
pub type Basis = [Point; 3];

pub fn bounds(triangles: &[Triangle]) -> (Point, Point) {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in triangles.iter().flatten() {
        for axis in 0..3 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
    }
    (low, high)
}

pub fn difference(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}

pub fn view_basis(triangles: &[Triangle], signature: &str) -> (String, Basis) {
    let (x, y, z) = ([1., 0., 0.], [0., 1., 0.], [0., 0., 1.]);
    if signature == "ARMO" {
        return ("front".into(), [[-1., 0., 0.], y, z]);
    }
    let (low, high) = bounds(triangles);
    let extent = difference(high, low);
    let mut sorted = extent;
    sorted.sort_by(f64::total_cmp);
    let mut area = [0.; 3];
    for triangle in triangles {
        let a = difference(triangle[1], triangle[0]);
        let b = difference(triangle[2], triangle[0]);
        for axis in 0..3 {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            area[axis] += (a[u] * b[v] - a[v] * b[u]).abs();
        }
    }
    let along_y = area[1] >= area[0];
    let flat = extent[2] < 0.35 * sorted[1] && area[2] > area[if along_y { 1 } else { 0 }];
    if signature == "WEAP" && !flat {
        return (
            "diagonal".into(),
            diagonal(if extent[0] >= extent[1] {
                [x, y, z]
            } else {
                [y, x, z]
            }),
        );
    }
    let (mut name, mut basis) = if flat {
        let basis = if extent[0] >= extent[1] {
            [x, z, y]
        } else {
            [y, z, [-1., 0., 0.]]
        };
        if signature == "WEAP" {
            return ("top-diagonal".into(), diagonal(basis));
        }
        ("top".to_string(), basis)
    } else {
        let [right, toward, up] = if along_y {
            [x, [0., -1., 0.], z]
        } else {
            [[0., -1., 0.], [-1., 0., 0.], z]
        };
        let angle = 20f64.to_radians();
        (
            "side".into(),
            [
                right,
                std::array::from_fn(|i| toward[i] * angle.cos() + up[i] * angle.sin()),
                std::array::from_fn(|i| up[i] * angle.cos() - toward[i] * angle.sin()),
            ],
        )
    };
    let longest = (0..3)
        .max_by(|&a, &b| extent[a].total_cmp(&extent[b]).then(b.cmp(&a)))
        .unwrap();
    if longest != 2 && extent[longest] >= 3. * sorted[1] {
        name.push_str("-diagonal");
        basis = diagonal(basis);
    } else if signature == "ALCH" {
        // Aid and consumables lie on the square's diagonal like weapons, longest side bottom-left to top-right:
        // a standing bottle or can leans its top that way.
        name.push_str("-diagonal");
        basis = if !flat && longest == 2 { leaning(basis) } else { diagonal(basis) };
    }
    (name, basis)
}

// Screen right turns to point up-right.
fn diagonal([right, toward, up]: Basis) -> Basis {
    [
        std::array::from_fn(|i| (right[i] - up[i]) / 2f64.sqrt()),
        toward,
        std::array::from_fn(|i| (right[i] + up[i]) / 2f64.sqrt()),
    ]
}

// Screen up turns to point up-right.
fn leaning([right, toward, up]: Basis) -> Basis {
    [
        std::array::from_fn(|i| (right[i] + up[i]) / 2f64.sqrt()),
        toward,
        std::array::from_fn(|i| (up[i] - right[i]) / 2f64.sqrt()),
    ]
}

pub fn item_icon(triangles: &[Triangle], signature: &str) -> Result<(String, Vec<u8>), String> {
    let (view, basis) = view_basis(triangles, signature);
    let bytes: Vec<u8> = triangles
        .iter()
        .flatten()
        .flat_map(|point| {
            basis.iter().flat_map(move |axis| {
                // Match the former BLAS projection, including coplanar depth ties.
                point[2]
                    .mul_add(axis[2], point[1].mul_add(axis[1], point[0] * axis[0]))
                    .to_le_bytes()
            })
        })
        .collect();
    let (covered, depth, normals) = crate::ui_icon::front_view(&bytes)?;
    let read = |bytes: &[u8], index: usize| {
        f64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    };
    let mut high = vec![0u8; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            let at = y * 256 + x;
            if covered[at] == 0 {
                continue;
            }
            let n: Point = std::array::from_fn(|i| read(&normals, at * 3 + i));
            let mut line = false;
            for (dx, dy) in [(2i32, 0i32), (-2, 0), (0, 2), (0, -2)] {
                let other = ((y as i32 + dy).rem_euclid(256) * 256
                    + (x as i32 + dx).rem_euclid(256)) as usize;
                if covered[other] == 0 {
                    continue;
                }
                let m: Point = std::array::from_fn(|i| read(&normals, other * 3 + i));
                let delta = read(&depth, other) - read(&depth, at);
                let planes = (delta * n[2] + dx as f64 * n[0] + dy as f64 * n[1]).abs() > 4.
                    && (delta * m[2] + dx as f64 * m[0] + dy as f64 * m[1]).abs() > 4.;
                line |= n[0] * m[0] + n[1] * m[1] + n[2] * m[2] < 0.85 || planes;
            }
            high[at] = if line { 40 } else { 255 };
        }
    }
    let mut alpha = Vec::with_capacity(128 * 128);
    for y in 0..128 {
        for x in 0..128 {
            let at = y * 512 + x * 2;
            let sum = high[at] as u32
                + high[at + 1] as u32
                + high[at + 256] as u32
                + high[at + 257] as u32;
            alpha.push((sum as f64 / 4.).round_ties_even() as u8);
        }
    }
    Ok((view, alpha))
}
