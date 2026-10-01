//! OpenGL 1.2.1 fixed-function matrix stacks and column-major transforms.
//! https://registry.khronos.org/OpenGL/specs/gl/glspec121.pdf

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassicGlMatrix(pub [f64; 16]);

impl ClassicGlMatrix {
    pub const IDENTITY: Self = Self([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);

    pub fn multiply(self, rhs: Self) -> Self {
        let mut result = [0.0; 16];
        for column in 0..4 {
            for row in 0..4 {
                result[column * 4 + row] = (0..4)
                    .map(|inner| self.0[inner * 4 + row] * rhs.0[column * 4 + inner])
                    .sum();
            }
        }
        Self(result)
    }

    pub fn transform(self, point: [f64; 4]) -> [f64; 4] {
        std::array::from_fn(|row| {
            (0..4)
                .map(|column| self.0[column * 4 + row] * point[column])
                .sum()
        })
    }

    fn translation(x: f64, y: f64, z: f64) -> Self {
        let mut result = Self::IDENTITY;
        result.0[12..15].copy_from_slice(&[x, y, z]);
        result
    }

    fn scale(x: f64, y: f64, z: f64) -> Self {
        let mut result = Self::IDENTITY;
        result.0[0] = x;
        result.0[5] = y;
        result.0[10] = z;
        result
    }

    fn rotation(angle_degrees: f64, x: f64, y: f64, z: f64) -> Option<Self> {
        let length = x.hypot(y).hypot(z);
        if length == 0.0 || !length.is_finite() {
            return None;
        }
        let (x, y, z) = (x / length, y / length, z / length);
        let (sine, cosine) = angle_degrees.to_radians().sin_cos();
        let one_minus = 1.0 - cosine;
        Some(Self([
            x * x * one_minus + cosine,
            y * x * one_minus + z * sine,
            z * x * one_minus - y * sine,
            0.0,
            x * y * one_minus - z * sine,
            y * y * one_minus + cosine,
            z * y * one_minus + x * sine,
            0.0,
            x * z * one_minus + y * sine,
            y * z * one_minus - x * sine,
            z * z * one_minus + cosine,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ]))
    }

    fn ortho(left: f64, right: f64, bottom: f64, top: f64, near: f64, far: f64) -> Option<Self> {
        let (dx, dy, dz) = (right - left, top - bottom, far - near);
        if dx == 0.0 || dy == 0.0 || dz == 0.0 {
            return None;
        }
        Some(Self([
            2.0 / dx,
            0.0,
            0.0,
            0.0,
            0.0,
            2.0 / dy,
            0.0,
            0.0,
            0.0,
            0.0,
            -2.0 / dz,
            0.0,
            -(right + left) / dx,
            -(top + bottom) / dy,
            -(far + near) / dz,
            1.0,
        ]))
    }

    fn frustum(left: f64, right: f64, bottom: f64, top: f64, near: f64, far: f64) -> Option<Self> {
        let (dx, dy, dz) = (right - left, top - bottom, far - near);
        if near <= 0.0 || far <= 0.0 || dx == 0.0 || dy == 0.0 || dz == 0.0 {
            return None;
        }
        Some(Self([
            2.0 * near / dx,
            0.0,
            0.0,
            0.0,
            0.0,
            2.0 * near / dy,
            0.0,
            0.0,
            (right + left) / dx,
            (top + bottom) / dy,
            -(far + near) / dz,
            -1.0,
            0.0,
            0.0,
            -2.0 * far * near / dz,
            0.0,
        ]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassicGlMatrixMode {
    ModelView,
    Projection,
    Texture,
}

#[derive(Debug, Clone)]
pub struct ClassicGlTransform {
    mode: ClassicGlMatrixMode,
    modelview: Vec<ClassicGlMatrix>,
    projection: Vec<ClassicGlMatrix>,
    texture: Vec<ClassicGlMatrix>,
}

impl Default for ClassicGlTransform {
    fn default() -> Self {
        Self {
            mode: ClassicGlMatrixMode::ModelView,
            modelview: vec![ClassicGlMatrix::IDENTITY],
            projection: vec![ClassicGlMatrix::IDENTITY],
            texture: vec![ClassicGlMatrix::IDENTITY],
        }
    }
}

impl ClassicGlTransform {
    pub fn set_mode(&mut self, mode: u32) -> bool {
        self.mode = match mode {
            0x1700 => ClassicGlMatrixMode::ModelView,
            0x1701 => ClassicGlMatrixMode::Projection,
            0x1702 => ClassicGlMatrixMode::Texture,
            _ => return false,
        };
        true
    }

    fn stack_mut(&mut self) -> &mut Vec<ClassicGlMatrix> {
        match self.mode {
            ClassicGlMatrixMode::ModelView => &mut self.modelview,
            ClassicGlMatrixMode::Projection => &mut self.projection,
            ClassicGlMatrixMode::Texture => &mut self.texture,
        }
    }

    pub fn current(&self) -> ClassicGlMatrix {
        match self.mode {
            ClassicGlMatrixMode::ModelView => *self.modelview.last().unwrap(),
            ClassicGlMatrixMode::Projection => *self.projection.last().unwrap(),
            ClassicGlMatrixMode::Texture => *self.texture.last().unwrap(),
        }
    }

    pub fn push(&mut self) -> bool {
        let maximum = if self.mode == ClassicGlMatrixMode::ModelView {
            32
        } else {
            2
        };
        let stack = self.stack_mut();
        if stack.len() >= maximum {
            return false;
        }
        stack.push(*stack.last().unwrap());
        true
    }

    pub fn pop(&mut self) -> bool {
        let stack = self.stack_mut();
        if stack.len() <= 1 {
            return false;
        }
        stack.pop();
        true
    }

    pub fn load(&mut self, matrix: ClassicGlMatrix) {
        *self.stack_mut().last_mut().unwrap() = matrix;
    }

    pub fn multiply(&mut self, matrix: ClassicGlMatrix) {
        self.load(self.current().multiply(matrix));
    }

    pub fn translate(&mut self, x: f64, y: f64, z: f64) {
        self.multiply(ClassicGlMatrix::translation(x, y, z));
    }

    pub fn scale(&mut self, x: f64, y: f64, z: f64) {
        self.multiply(ClassicGlMatrix::scale(x, y, z));
    }

    pub fn rotate(&mut self, angle: f64, x: f64, y: f64, z: f64) -> bool {
        let Some(matrix) = ClassicGlMatrix::rotation(angle, x, y, z) else {
            return false;
        };
        self.multiply(matrix);
        true
    }

    pub fn ortho(&mut self, bounds: [f64; 6]) -> bool {
        let Some(matrix) = ClassicGlMatrix::ortho(
            bounds[0], bounds[1], bounds[2], bounds[3], bounds[4], bounds[5],
        ) else {
            return false;
        };
        self.multiply(matrix);
        true
    }

    pub fn frustum(&mut self, bounds: [f64; 6]) -> bool {
        let Some(matrix) = ClassicGlMatrix::frustum(
            bounds[0], bounds[1], bounds[2], bounds[3], bounds[4], bounds[5],
        ) else {
            return false;
        };
        self.multiply(matrix);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_order_is_column_major_post_multiplication() {
        let mut state = ClassicGlTransform::default();
        state.translate(2.0, 3.0, 4.0);
        state.scale(5.0, 6.0, 7.0);
        assert_eq!(
            state.current().transform([1.0, 1.0, 1.0, 1.0]),
            [7.0, 9.0, 11.0, 1.0]
        );
        assert!(state.push());
        assert!(state.rotate(90.0, 0.0, 0.0, 1.0));
        assert!(state.pop());
        assert_eq!(
            state.current().transform([1.0, 1.0, 1.0, 1.0]),
            [7.0, 9.0, 11.0, 1.0]
        );
        assert!(!state.pop());
    }

    #[test]
    fn projection_stack_is_independent_and_has_two_entries() {
        let mut state = ClassicGlTransform::default();
        assert!(state.set_mode(0x1701));
        assert!(state.ortho([-1.0, 1.0, -1.0, 1.0, 1.0, 11.0]));
        assert_eq!(
            state.current().transform([0.0, 0.0, -1.0, 1.0]),
            [0.0, 0.0, -1.0, 1.0]
        );
        assert!(state.push());
        assert!(!state.push());
        assert!(state.frustum([-1.0, 1.0, -1.0, 1.0, 1.0, 11.0]));
        assert!(state.pop());
        assert!(state.set_mode(0x1700));
        assert_eq!(state.current(), ClassicGlMatrix::IDENTITY);
    }
}
