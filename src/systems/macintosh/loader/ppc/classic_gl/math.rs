//! Column-major 4x4 matrices and the OpenGL 1.2 matrix constructors
//! (OpenGL 1.2.1 specification, section 2.10.2).

pub type Vec4 = [f32; 4];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);

    #[inline]
    pub fn at(&self, row: usize, column: usize) -> f32 {
        self.0[column * 4 + row]
    }

    pub fn mul(&self, other: &Self) -> Self {
        let mut out = [0.0; 16];
        for column in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += self.at(row, k) * other.at(k, column);
                }
                out[column * 4 + row] = sum;
            }
        }
        Self(out)
    }

    #[inline]
    pub fn transform(&self, v: Vec4) -> Vec4 {
        let m = &self.0;
        [
            m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12] * v[3],
            m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13] * v[3],
            m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14] * v[3],
            m[3] * v[0] + m[7] * v[1] + m[11] * v[2] + m[15] * v[3],
        ]
    }

    /// Transform a direction by the upper 3x3 of the matrix.
    #[inline]
    pub fn transform_direction(&self, v: [f32; 3]) -> [f32; 3] {
        let m = &self.0;
        [
            m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
            m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
            m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
        ]
    }

    pub fn transpose(&self) -> Self {
        let mut out = [0.0; 16];
        for row in 0..4 {
            for column in 0..4 {
                out[column * 4 + row] = self.at(column, row);
            }
        }
        Self(out)
    }

    /// General inverse by cofactors; singular matrices return the identity,
    /// matching implementations that leave normals untransformed.
    pub fn inverse(&self) -> Self {
        let m = &self.0;
        let mut inv = [0.0f32; 16];
        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
            + m[9] * m[7] * m[14]
            + m[13] * m[6] * m[11]
            - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
            - m[8] * m[7] * m[14]
            - m[12] * m[6] * m[11]
            + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
            + m[8] * m[7] * m[13]
            + m[12] * m[5] * m[11]
            - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
            - m[8] * m[6] * m[13]
            - m[12] * m[5] * m[10]
            + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
            - m[9] * m[3] * m[14]
            - m[13] * m[2] * m[11]
            + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
            + m[8] * m[3] * m[14]
            + m[12] * m[2] * m[11]
            - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
            - m[8] * m[3] * m[13]
            - m[12] * m[1] * m[11]
            + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
            + m[8] * m[2] * m[13]
            + m[12] * m[1] * m[10]
            - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
            + m[5] * m[3] * m[14]
            + m[13] * m[2] * m[7]
            - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
            - m[4] * m[3] * m[14]
            - m[12] * m[2] * m[7]
            + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
            + m[4] * m[3] * m[13]
            + m[12] * m[1] * m[7]
            - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
            - m[4] * m[2] * m[13]
            - m[12] * m[1] * m[6]
            + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
            - m[5] * m[3] * m[10]
            - m[9] * m[2] * m[7]
            + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
            + m[4] * m[3] * m[10]
            + m[8] * m[2] * m[7]
            - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
            - m[4] * m[3] * m[9]
            - m[8] * m[1] * m[7]
            + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
            + m[4] * m[2] * m[9]
            + m[8] * m[1] * m[6]
            - m[8] * m[2] * m[5];
        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if det == 0.0 || !det.is_finite() {
            return Self::IDENTITY;
        }
        let scale = 1.0 / det;
        for value in &mut inv {
            *value *= scale;
        }
        Self(inv)
    }

    pub fn translate(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.0[12] = x;
        m.0[13] = y;
        m.0[14] = z;
        m
    }

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.0[0] = x;
        m.0[5] = y;
        m.0[10] = z;
        m
    }

    /// glRotate: `angle` degrees about the axis (x, y, z).
    pub fn rotate(angle: f32, x: f32, y: f32, z: f32) -> Self {
        let length = (x * x + y * y + z * z).sqrt();
        if length == 0.0 {
            return Self::IDENTITY;
        }
        let (x, y, z) = (x / length, y / length, z / length);
        let (s, c) = angle.to_radians().sin_cos();
        let t = 1.0 - c;
        Self([
            x * x * t + c,
            y * x * t + z * s,
            x * z * t - y * s,
            0.0,
            x * y * t - z * s,
            y * y * t + c,
            y * z * t + x * s,
            0.0,
            x * z * t + y * s,
            y * z * t - x * s,
            z * z * t + c,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ])
    }

    pub fn ortho(l: f64, r: f64, b: f64, t: f64, n: f64, f: f64) -> Self {
        let mut m = Self::IDENTITY;
        m.0[0] = (2.0 / (r - l)) as f32;
        m.0[5] = (2.0 / (t - b)) as f32;
        m.0[10] = (-2.0 / (f - n)) as f32;
        m.0[12] = (-(r + l) / (r - l)) as f32;
        m.0[13] = (-(t + b) / (t - b)) as f32;
        m.0[14] = (-(f + n) / (f - n)) as f32;
        m
    }

    pub fn frustum(l: f64, r: f64, b: f64, t: f64, n: f64, f: f64) -> Self {
        let mut m = Self([0.0; 16]);
        m.0[0] = (2.0 * n / (r - l)) as f32;
        m.0[5] = (2.0 * n / (t - b)) as f32;
        m.0[8] = ((r + l) / (r - l)) as f32;
        m.0[9] = ((t + b) / (t - b)) as f32;
        m.0[10] = (-(f + n) / (f - n)) as f32;
        m.0[11] = -1.0;
        m.0[14] = (-2.0 * f * n / (f - n)) as f32;
        m
    }
}

#[inline]
pub fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let length = dot3(v, v).sqrt();
    if length == 0.0 {
        v
    } else {
        [v[0] / length, v[1] / length, v[2] / length]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_and_inverse_round_trip() {
        let m = Mat4::rotate(90.0, 0.0, 0.0, 1.0).mul(&Mat4::translate(1.0, 2.0, 3.0));
        let v = m.transform([1.0, 0.0, 0.0, 1.0]);
        assert!((v[0] - -2.0).abs() < 1e-5 && (v[1] - 2.0).abs() < 1e-5);
        let back = m.inverse().transform(v);
        assert!((back[0] - 1.0).abs() < 1e-5 && back[1].abs() < 1e-5);
    }

    #[test]
    fn frustum_maps_near_plane_to_minus_one() {
        let m = Mat4::frustum(-1.0, 1.0, -1.0, 1.0, 1.0, 10.0);
        let v = m.transform([0.0, 0.0, -1.0, 1.0]);
        assert!((v[2] / v[3] - -1.0).abs() < 1e-5);
        let v = m.transform([0.0, 0.0, -10.0, 1.0]);
        assert!((v[2] / v[3] - 1.0).abs() < 1e-5);
    }
}
