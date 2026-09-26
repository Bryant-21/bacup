//! Float32 arithmetic in the exact operation order CK 1.11.137 uses.
//!
//! Every helper rounds its operands and result to binary32, mirroring the
//! research implementation that was proved byte-identical against CK output.
//! Keep the order of operations unchanged: reordering changes scene bytes.

pub type Vec3 = [f32; 3];
/// Row-major rotation: `rows[i]` is `(m1i, m2i, m3i)` in NIF field naming.
pub type Mat3 = [[f32; 3]; 3];

pub const FLOAT_MAX: f32 = f32::MAX;

#[inline]
pub fn add(a: f32, b: f32) -> f32 {
    a + b
}

#[inline]
pub fn mul(a: f32, b: f32) -> f32 {
    a * b
}

/// Keeps the exact product in double precision before one final rounding,
/// matching the fused multiply-add CK uses when merging bounding spheres.
#[inline]
pub fn fma_rounded(a: f32, b: f32, c: f32) -> f32 {
    ((a as f64) * (b as f64) + c as f64) as f32
}

#[inline]
pub fn dot(row: &Vec3, point: &Vec3) -> f32 {
    add(add(mul(row[0], point[0]), mul(row[1], point[1])), mul(row[2], point[2]))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub rotation: Mat3,
    pub translation: Vec3,
    pub scale: f32,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0, 0.0, 0.0],
        scale: 1.0,
    };

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    pub fn apply(&self, point: &Vec3) -> Vec3 {
        let mut out = [0.0; 3];
        for (index, row) in self.rotation.iter().enumerate() {
            out[index] = add(mul(dot(row, point), self.scale), self.translation[index]);
        }
        out
    }

    pub fn inverse(&self) -> Result<Transform, String> {
        if self.scale == 0.0 {
            return Err("direct reference has zero scale".into());
        }
        let r = &self.rotation;
        let inverse_rotation = [
            [r[0][0], r[1][0], r[2][0]],
            [r[0][1], r[1][1], r[2][1]],
            [r[0][2], r[1][2], r[2][2]],
        ];
        let inverse_scale = 1.0f32 / self.scale;
        let negative = [-self.translation[0], -self.translation[1], -self.translation[2]];
        let mut inverse_translation = [0.0; 3];
        for (index, row) in inverse_rotation.iter().enumerate() {
            inverse_translation[index] = mul(dot(row, &negative), inverse_scale);
        }
        Ok(Transform {
            rotation: inverse_rotation,
            translation: inverse_translation,
            scale: inverse_scale,
        })
    }

    /// `self` applied after `child`: CK's parent-by-child composition order.
    pub fn compose(&self, child: &Transform) -> Transform {
        let mut rotation = [[0.0; 3]; 3];
        for (row, parent_row) in self.rotation.iter().enumerate() {
            for column in 0..3 {
                let child_column = [child.rotation[0][column], child.rotation[1][column], child.rotation[2][column]];
                rotation[row][column] = dot(parent_row, &child_column);
            }
        }
        let mut translation = [0.0; 3];
        for (index, row) in self.rotation.iter().enumerate() {
            translation[index] = add(mul(dot(row, &child.translation), self.scale), self.translation[index]);
        }
        Transform {
            rotation,
            translation,
            scale: mul(self.scale, child.scale),
        }
    }

    /// The 3x4 row-major matrix Umbra stores for an instance.
    pub fn scene_matrix(&self) -> [f32; 12] {
        let mut out = [0.0; 12];
        for (index, row) in self.rotation.iter().enumerate() {
            out[index * 4] = mul(row[0], self.scale);
            out[index * 4 + 1] = mul(row[1], self.scale);
            out[index * 4 + 2] = mul(row[2], self.scale);
            out[index * 4 + 3] = self.translation[index];
        }
        out
    }
}

pub fn apply_chain(point: &Vec3, transforms: &[Transform]) -> Vec3 {
    let mut current = *point;
    for transform in transforms {
        current = transform.apply(&current);
    }
    current
}

/// CK's float32 sine/cosine polynomial pair.
pub fn sin_cos(value: f32) -> (f32, f32) {
    let quotient = mul(value, 0.15915494309189535_f64 as f32);
    let quotient = add(quotient, if value >= 0.0 { 0.5 } else { -0.5 }) as i64 as f32;
    let mut y = add(value, -mul(std::f64::consts::TAU as f32, quotient));
    let mut cosine_sign = 1.0f32;
    let half_pi = std::f64::consts::FRAC_PI_2 as f32;
    if y > half_pi {
        y = add(std::f64::consts::PI as f32, -y);
        cosine_sign = -1.0;
    } else if y < -half_pi {
        y = add(-(std::f64::consts::PI as f32), -y);
        cosine_sign = -1.0;
    }

    let y2 = mul(y, y);
    let mut sine = add(mul(-2.3889859e-08_f64 as f32, y2), 2.7525562e-06_f64 as f32);
    sine = add(mul(sine, y2), -0.00019840874_f64 as f32);
    sine = add(mul(sine, y2), 0.008333331_f64 as f32);
    sine = add(mul(sine, y2), -0.16666667_f64 as f32);
    sine = add(mul(sine, y2), 1.0);
    sine = mul(sine, y);

    let mut cosine = add(mul(-2.6051615e-07_f64 as f32, y2), 2.4760495e-05_f64 as f32);
    cosine = add(mul(cosine, y2), -0.0013888378_f64 as f32);
    cosine = add(mul(cosine, y2), 0.041666638_f64 as f32);
    cosine = add(mul(cosine, y2), -0.5);
    cosine = add(mul(cosine, y2), 1.0);
    (sine, mul(cosine, cosine_sign))
}

/// REFR Euler angles (radians, XYZ) to the NIF-row rotation CK composes.
pub fn euler_rotation(euler: Vec3) -> Mat3 {
    let (sine_x, cosine_x) = sin_cos(euler[0]);
    let (sine_y, cosine_y) = sin_cos(euler[1]);
    let (sine_z, cosine_z) = sin_cos(euler[2]);
    let rotation = [
        [
            mul(cosine_z, cosine_y),
            add(mul(mul(cosine_z, sine_y), sine_x), -mul(sine_z, cosine_x)),
            add(mul(mul(cosine_z, sine_y), cosine_x), mul(sine_z, sine_x)),
        ],
        [
            mul(sine_z, cosine_y),
            add(mul(mul(sine_z, sine_y), sine_x), mul(cosine_z, cosine_x)),
            add(mul(mul(sine_z, sine_y), cosine_x), -mul(cosine_z, sine_x)),
        ],
        [-sine_y, mul(cosine_y, sine_x), mul(cosine_y, cosine_x)],
    ];
    [
        [rotation[0][0], rotation[1][0], rotation[2][0]],
        [rotation[0][1], rotation[1][1], rotation[2][1]],
        [rotation[0][2], rotation[1][2], rotation[2][2]],
    ]
}
