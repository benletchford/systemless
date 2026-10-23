//! The OpenGL Utility library entry points exported by `OpenGLUtility`.
//! Signatures follow Apple's glu.h (Mac OS X 10.2.8 SDK); behaviour follows
//! the GLU 1.3 specification.
//! <https://registry.khronos.org/OpenGL/specs/gl/glu1.3.pdf>

use super::args::PpcCallArgs;
use super::consts::*;
use super::math::{normalize3, Mat4};
use super::texture::{BaseFormat, PixelLayout, PixelStore, MAX_TEXTURE_DIMENSION};
use super::{ClassicGlFramebuffer, GlCall, GlContext, GlResult, PpcSectionMem};
use ppc::PpcCpu;
use std::rc::Rc;

const GLU_INVALID_ENUM: u32 = 100_900;
const GLU_INVALID_VALUE: u32 = 100_901;
const GLU_OUT_OF_MEMORY: u32 = 100_902;
const GLU_STRING_OFFSET: u32 = 0x300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GluFunction {
    Perspective,
    Ortho2D,
    LookAt,
    ErrorString,
    Build2DMipmaps,
    ScaleImage,
}

impl GluFunction {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "gluPerspective" => Self::Perspective,
            "gluOrtho2D" => Self::Ortho2D,
            "gluLookAt" => Self::LookAt,
            "gluErrorString" => Self::ErrorString,
            "gluBuild2DMipmaps" => Self::Build2DMipmaps,
            "gluScaleImage" => Self::ScaleImage,
            _ => return None,
        })
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// gluLookAt (GLU 1.3, section 4.2).
pub fn look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> Mat4 {
    let f = normalize3([center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]]);
    let s = normalize3(cross(f, normalize3(up)));
    let u = cross(s, f);
    let rotation = Mat4([
        s[0], u[0], -f[0], 0.0, //
        s[1], u[1], -f[1], 0.0, //
        s[2], u[2], -f[2], 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);
    rotation.mul(&Mat4::translate(-eye[0], -eye[1], -eye[2]))
}

/// gluPerspective (GLU 1.3, section 4.1).
pub fn perspective(fovy: f64, aspect: f64, near: f64, far: f64) -> Option<Mat4> {
    let f = 1.0 / (fovy.to_radians() / 2.0).tan();
    if aspect == 0.0 || near == far || !f.is_finite() {
        return None;
    }
    let mut m = Mat4([0.0; 16]);
    m.0[0] = (f / aspect) as f32;
    m.0[5] = f as f32;
    m.0[10] = ((far + near) / (near - far)) as f32;
    m.0[11] = -1.0;
    m.0[14] = (2.0 * far * near / (near - far)) as f32;
    Some(m)
}

/// Box-filter or point-sample an RGBA image to a new size (GLU 1.3,
/// section 3.1, gluScaleImage).
pub fn scale_rgba(
    pixels: &[[f32; 4]],
    width: usize,
    height: usize,
    new_width: usize,
    new_height: usize,
) -> Vec<[f32; 4]> {
    let mut out = Vec::with_capacity(new_width * new_height);
    let sx = width as f32 / new_width as f32;
    let sy = height as f32 / new_height as f32;
    for y in 0..new_height {
        let y0 = (y as f32 * sy) as usize;
        let y1 = (((y + 1) as f32 * sy).ceil() as usize).clamp(y0 + 1, height);
        for x in 0..new_width {
            let x0 = (x as f32 * sx) as usize;
            let x1 = (((x + 1) as f32 * sx).ceil() as usize).clamp(x0 + 1, width);
            let mut sum = [0.0; 4];
            let mut count = 0.0;
            for row in y0..y1 {
                for column in x0..x1 {
                    let p = pixels[row * width + column];
                    for c in 0..4 {
                        sum[c] += p[c];
                    }
                    count += 1.0;
                }
            }
            out.push(sum.map(|v| v / count));
        }
    }
    out
}

fn largest_power_of_two_at_most(value: u32) -> u32 {
    if value == 0 {
        0
    } else {
        (1u32 << (31 - value.leading_zeros())).min(MAX_TEXTURE_DIMENSION)
    }
}

pub fn call(
    ctx: &mut GlContext,
    mut framebuffer: Option<&mut ClassicGlFramebuffer>,
    function: GluFunction,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> GlResult {
    let mut args = PpcCallArgs::new(cpu, memory);
    match function {
        GluFunction::Perspective => {
            let (fovy, aspect, near, far) = (args.f64(), args.f64(), args.f64(), args.f64());
            if let Some(matrix) = perspective(fovy, aspect, near, far) {
                ctx.submit(GlCall::MultMatrix(matrix), &mut framebuffer);
            }
            GlResult::Void
        }
        GluFunction::Ortho2D => {
            let (l, r, b, t) = (args.f64(), args.f64(), args.f64(), args.f64());
            if l != r && b != t {
                ctx.submit(
                    GlCall::MultMatrix(Mat4::ortho(l, r, b, t, -1.0, 1.0)),
                    &mut framebuffer,
                );
            }
            GlResult::Void
        }
        GluFunction::LookAt => {
            let v: [f32; 9] = std::array::from_fn(|_| args.f64() as f32);
            let matrix = look_at([v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]);
            ctx.submit(GlCall::MultMatrix(matrix), &mut framebuffer);
            GlResult::Void
        }
        GluFunction::ErrorString => {
            let code = args.u32();
            let text = match code {
                NO_ERROR => "no error",
                INVALID_ENUM | GLU_INVALID_ENUM => "invalid enumerant",
                INVALID_VALUE | GLU_INVALID_VALUE => "invalid value",
                INVALID_OPERATION => "invalid operation",
                STACK_OVERFLOW => "stack overflow",
                STACK_UNDERFLOW => "stack underflow",
                OUT_OF_MEMORY | GLU_OUT_OF_MEMORY => "out of memory",
                _ => return GlResult::Value(0),
            };
            GlResult::Value(super::write_gl_string(
                args.memory(),
                GLU_STRING_OFFSET,
                text,
            ))
        }
        GluFunction::Build2DMipmaps => {
            let target = args.u32();
            let internal = args.u32();
            let width = args.i32();
            let height = args.i32();
            let format = args.u32();
            let kind = args.u32();
            let pointer = args.u32();
            if target != TEXTURE_2D || BaseFormat::from_internal(internal).is_none() {
                return GlResult::Value(GLU_INVALID_ENUM);
            }
            if width < 1 || height < 1 {
                return GlResult::Value(GLU_INVALID_VALUE);
            }
            let pixels = match super::unpack_image(
                ctx,
                args.memory(),
                width as u32,
                height as u32,
                format,
                kind,
                pointer,
            ) {
                Ok(Some(pixels)) => pixels,
                Ok(None) => return GlResult::Value(GLU_INVALID_VALUE),
                Err(_) => return GlResult::Value(GLU_INVALID_ENUM),
            };
            let (mut w, mut h) = (
                largest_power_of_two_at_most(width as u32),
                largest_power_of_two_at_most(height as u32),
            );
            let mut level_pixels = scale_rgba(
                &pixels,
                width as usize,
                height as usize,
                w as usize,
                h as usize,
            );
            let mut level = 0;
            loop {
                ctx.submit(
                    GlCall::TexImage2D {
                        level,
                        internal,
                        width: w,
                        height: h,
                        pixels: Some(Rc::new(level_pixels.clone())),
                    },
                    &mut framebuffer,
                );
                if w == 1 && h == 1 {
                    break;
                }
                let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
                level_pixels = scale_rgba(
                    &level_pixels,
                    w as usize,
                    h as usize,
                    nw as usize,
                    nh as usize,
                );
                w = nw;
                h = nh;
                level += 1;
            }
            GlResult::Value(0)
        }
        GluFunction::ScaleImage => {
            let format = args.u32();
            let width_in = args.i32();
            let height_in = args.i32();
            let kind_in = args.u32();
            let data_in = args.u32();
            let width_out = args.i32();
            let height_out = args.i32();
            let kind_out = args.u32();
            let data_out = args.u32();
            if width_in < 0 || height_in < 0 || width_out < 0 || height_out < 0 {
                return GlResult::Value(GLU_INVALID_VALUE);
            }
            if PixelLayout::new(format, kind_in).is_none()
                || PixelLayout::new(format, kind_out).is_none()
            {
                return GlResult::Value(GLU_INVALID_ENUM);
            }
            let pixels = match super::unpack_image(
                ctx,
                args.memory(),
                width_in as u32,
                height_in as u32,
                format,
                kind_in,
                data_in,
            ) {
                Ok(Some(pixels)) => pixels,
                _ => return GlResult::Value(GLU_INVALID_VALUE),
            };
            let scaled = scale_rgba(
                &pixels,
                width_in as usize,
                height_in as usize,
                width_out as usize,
                height_out as usize,
            );
            let store: PixelStore = ctx.state.client.pack;
            if !super::pack_pixels(
                args.memory(),
                data_out,
                width_out as usize,
                height_out as usize,
                format,
                kind_out,
                &store,
                &scaled,
            ) {
                return GlResult::Value(GLU_OUT_OF_MEMORY);
            }
            GlResult::Value(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perspective_matches_the_equivalent_frustum() {
        let m = perspective(90.0, 1.0, 1.0, 10.0).unwrap();
        let f = Mat4::frustum(-1.0, 1.0, -1.0, 1.0, 1.0, 10.0);
        for (a, b) in m.0.iter().zip(f.0.iter()) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn look_at_maps_the_eye_to_the_origin_looking_down_minus_z() {
        let m = look_at([0.0, 0.0, 5.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let eye = m.transform([0.0, 0.0, 5.0, 1.0]);
        assert!(eye[0].abs() < 1e-5 && eye[1].abs() < 1e-5 && eye[2].abs() < 1e-5);
        let center = m.transform([0.0, 0.0, 0.0, 1.0]);
        assert!((center[2] - -5.0).abs() < 1e-5);
    }

    #[test]
    fn scaling_averages_source_texels() {
        let pixels = vec![[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]];
        assert_eq!(scale_rgba(&pixels, 2, 1, 1, 1), vec![[0.5, 0.0, 0.5, 1.0]]);
    }
}
