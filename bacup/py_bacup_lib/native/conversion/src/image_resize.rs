// Pillow-compatible Lanczos resampling and premultiplied-alpha rounding.
// Reference: Pillow 12.1.1 libImaging/{Resample,Convert}.c.
// Copyright and permission notice: image_resize.Pillow-LICENSE.txt.

fn coefficients(input: usize, output: usize) -> Vec<(usize, Vec<i32>)> {
    let scale = input as f64 / output as f64;
    let filter_scale = scale.max(1.0);
    let support = 3.0 * filter_scale;
    (0..output).map(|at| {
        let center = (at as f64 + 0.5) * scale;
        let start = ((center - support + 0.5) as isize).max(0) as usize;
        let end = ((center + support + 0.5) as usize).min(input);
        let sinc = |x: f64| if x == 0.0 { 1.0 } else {
            let angle = x * std::f64::consts::PI;
            angle.sin() / angle
        };
        let weights: Vec<_> = (start..end).map(|x| {
            let x = (x as f64 - center + 0.5) / filter_scale;
            if (-3.0..3.0).contains(&x) { sinc(x) * sinc(x / 3.0) } else { 0.0 }
        }).collect();
        let sum: f64 = weights.iter().sum();
        (start, weights.into_iter().map(|w| {
            let w = if sum != 0.0 { w / sum } else { w };
            (w * (1 << 22) as f64 + if w < 0.0 { -0.5 } else { 0.5 }) as i32
        }).collect())
    }).collect()
}

pub(crate) fn lanczos_rgba(mut pixels: Vec<u8>, width: usize, height: usize, out_width: usize, out_height: usize) -> Vec<u8> {
    assert_eq!(pixels.len(), width * height * 4);
    assert!(width > 0 && height > 0 && out_width > 0 && out_height > 0);
    if (width, height) == (out_width, out_height) { return pixels; }
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        for channel in &mut pixel[..3] {
            let product = u32::from(*channel) * alpha + 128;
            *channel = (((product >> 8) + product) >> 8) as u8;
        }
    }
    let horizontal = if width == out_width { pixels } else {
        let weights = coefficients(width, out_width);
        let mut output = vec![0u8; out_width * height * 4];
        for (y, row) in output.chunks_exact_mut(out_width * 4).enumerate() {
            for (x, (start, weights)) in weights.iter().enumerate() {
                let mut sums = [1i64 << 21; 4];
                for (offset, weight) in weights.iter().enumerate() {
                    let pixel = &pixels[(y * width + start + offset) * 4..][..4];
                    for c in 0..4 { sums[c] += i64::from(pixel[c]) * i64::from(*weight); }
                }
                for c in 0..4 { row[x * 4 + c] = (sums[c] >> 22).clamp(0, 255) as u8; }
            }
        }
        output
    };
    let mut output = if height == out_height { horizontal } else {
        let weights = coefficients(height, out_height);
        let mut output = vec![0u8; out_width * out_height * 4];
        for (y, (start, weights)) in weights.iter().enumerate() {
            for x in 0..out_width {
                let mut sums = [1i64 << 21; 4];
                for (offset, weight) in weights.iter().enumerate() {
                    let pixel = &horizontal[((start + offset) * out_width + x) * 4..][..4];
                    for c in 0..4 { sums[c] += i64::from(pixel[c]) * i64::from(*weight); }
                }
                for c in 0..4 { output[(y * out_width + x) * 4 + c] = (sums[c] >> 22).clamp(0, 255) as u8; }
            }
        }
        output
    };
    for pixel in output.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha != 0 && alpha != 255 {
            for channel in &mut pixel[..3] { *channel = (255 * u32::from(*channel) / alpha).min(255) as u8; }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    #[test]
    fn matches_pillow_for_transparency_edges_and_axis_changes() {
        let cases: serde_json::Value = serde_json::from_str(include_str!("test_fixtures/pipboy_map/lanczos.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let size = |name: &str| case[name].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect::<Vec<_>>();
            let input = size("input");
            let output = size("output");
            let pixels = case["pixels"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            let expected: Vec<_> = case["expected"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            assert_eq!(super::lanczos_rgba(pixels, input[0], input[1], output[0], output[1]), expected, "{input:?} -> {output:?}");
        }
    }
}
