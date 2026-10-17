const SIZE: usize = 256;
type Point = [f64; 3];

fn cross(a: Point, b: Point) -> Point {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn difference(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn front_view(data: &[u8]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), String> {
    if data.is_empty() || data.len() % 72 != 0 {
        return Err("Expected nonempty float64 triangles with shape (n, 3, 3)".into());
    }
    let mut points = Vec::with_capacity(data.len() / 24);
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for vertex in data.chunks_exact(24) {
        let read = |offset| f64::from_le_bytes(vertex[offset..offset + 8].try_into().unwrap());
        let point = [read(0), -read(16), read(8)];
        for axis in 0..3 {
            if !point[axis].is_finite() {
                return Err("Icon geometry contains nonfinite coordinates".into());
            }
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
        points.push(point);
    }
    let extent = difference(high, low);
    let scale = (224. / extent[0]).min(224. / extent[1]);
    if !scale.is_finite() {
        return Err("Icon geometry has no visible extent".into());
    }
    for point in &mut points {
        for axis in 0..3 {
            point[axis] = (point[axis] - low[axis]) * scale;
            if axis < 2 {
                point[axis] += (SIZE as f64 - extent[axis] * scale) / 2.;
            }
        }
    }
    let mut covered = vec![0u8; SIZE * SIZE];
    let mut depth = vec![f64::NEG_INFINITY; SIZE * SIZE];
    let mut normals = vec![[0.; 3]; SIZE * SIZE];
    for triangle in points.chunks_exact(3) {
        let mut normal = cross(difference(triangle[1], triangle[0]), difference(triangle[2], triangle[0]));
        let area = normal[2];
        if area.abs() < 0.0001 {
            continue;
        }
        let norm = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        let factor = area.signum() / norm;
        for value in &mut normal {
            *value *= factor;
        }
        let min = |axis: usize| triangle.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min).floor().max(0.) as usize;
        let max = |axis: usize| triangle.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max).ceil().min((SIZE - 1) as f64) as usize;
        for y in min(1)..=max(1) {
            for x in min(0)..=max(0) {
                let mut edges = [0.; 3];
                for i in 0..3 {
                    let a = triangle[i];
                    let b = triangle[(i + 1) % 3];
                    edges[i] = (b[0] - a[0]) * (y as f64 + 0.5 - a[1]) - (b[1] - a[1]) * (x as f64 + 0.5 - a[0]);
                }
                if !(edges.iter().all(|e| *e >= 0.) || edges.iter().all(|e| *e <= 0.)) {
                    continue;
                }
                let value = (edges[1] * triangle[0][2] + edges[2] * triangle[1][2] + edges[0] * triangle[2][2]) / area;
                let pixel = y * SIZE + x;
                if value > depth[pixel] {
                    depth[pixel] = value;
                    normals[pixel] = normal;
                    covered[pixel] = 1;
                }
            }
        }
    }
    let depth_bytes = depth.iter().zip(&covered).flat_map(|(&value, &visible)| {
        (if visible != 0 { value } else { 0. }).to_le_bytes()
    }).collect();
    let normal_bytes = normals.iter().flatten().flat_map(|value| value.to_le_bytes()).collect();
    Ok((covered, depth_bytes, normal_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_depth_wins_independent_of_winding_and_submission_order() {
        let triangle = [[-1., 0., -1.], [1., 0., -1.], [0., 0., 1.]];
        let mut front = triangle;
        for point in &mut front { point[1] = 1.; }
        front.reverse();
        let bytes = |triangles: &[Point]| triangles.iter().flatten().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>();
        let first = front_view(&bytes(&[triangle, front].concat())).unwrap();
        let second = front_view(&bytes(&[front, triangle].concat())).unwrap();
        assert_eq!(first, second);
        let pixel = 128 * SIZE + 128;
        assert_eq!(first.0[pixel], 1);
        assert_eq!(f64::from_le_bytes(first.1[pixel * 8..pixel * 8 + 8].try_into().unwrap()), 112.);
        assert_eq!(first.0[0], 0);
    }

    #[test]
    fn rejects_invalid_geometry() {
        assert!(front_view(&[]).is_err());
        assert!(front_view(&[0; 71]).is_err());
        assert!(front_view(&[0; 72]).is_err());
        assert!(front_view(&f64::NAN.to_le_bytes().repeat(9)).is_err());
    }
}
