//! Binary messages between the emulation worker and the GPU compute worker.
//!
//! Requests carry complete snapshots of guest pixels, so the compute worker
//! never observes guest memory. Struct encodings destructure every field
//! without `..`: adding a field to a request type fails to compile here
//! instead of silently dropping data at the worker boundary.
use systemless::systems::macintosh::loader::ppc::{
    PpcQ3FogStyleData, PpcQ3GpuTexture, PpcQ3LightData, PpcQ3LightKind, PpcQ3LightRecord,
    PpcQ3RasterDraw, PpcQ3RasterFrame, PpcQ3RasterLighting, PpcQ3RasterPrimitive,
    PpcQ3RasterShading, PpcQ3RasterVertex,
};
use systemless::systems::macintosh::quickdraw::gpu::{
    QuickDrawGpuArithmetic, QuickDrawGpuBlit, QuickDrawGpuBlitClip, QuickDrawGpuCommand,
    QuickDrawGpuDeepMask, QuickDrawGpuDirectColor, QuickDrawGpuMask, QuickDrawGpuPackedCopy,
    QuickDrawGpuPixel,
};

/// Bumped whenever an encoding changes; both workers load the same bundle,
/// but a stale cached worker script must not misinterpret requests.
pub const WIRE_VERSION: u8 = 1;

#[derive(Debug, PartialEq, Eq)]
pub struct WireError(pub &'static str);

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GPU compute message: {}", self.0)
    }
}

pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], WireError> {
        if count > self.bytes.len() {
            return Err(WireError("truncated"));
        }
        let (head, tail) = self.bytes.split_at(count);
        self.bytes = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    pub fn finish(self) -> Result<(), WireError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(WireError("trailing bytes"))
        }
    }
}

pub trait Wire: Sized {
    /// Smallest encoding, bounding sequence lengths before allocation.
    const MIN_BYTES: usize;
    fn put(&self, out: &mut Vec<u8>);
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError>;

    fn put_slice(values: &[Self], out: &mut Vec<u8>) {
        out.reserve(values.len().saturating_mul(Self::MIN_BYTES));
        for value in values {
            value.put(out);
        }
    }

    fn get_vec(input: &mut Reader<'_>, len: usize) -> Result<Vec<Self>, WireError> {
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(Self::get(input)?);
        }
        Ok(values)
    }
}

fn put_sequence<T: Wire>(values: &[T], out: &mut Vec<u8>) {
    u32::try_from(values.len())
        .expect("GPU compute sequence exceeds u32")
        .put(out);
    T::put_slice(values, out);
}

impl Wire for u8 {
    const MIN_BYTES: usize = 1;
    fn put(&self, out: &mut Vec<u8>) {
        out.push(*self);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(input.take(1)?[0])
    }
    fn put_slice(values: &[Self], out: &mut Vec<u8>) {
        out.extend_from_slice(values);
    }
    fn get_vec(input: &mut Reader<'_>, len: usize) -> Result<Vec<Self>, WireError> {
        Ok(input.take(len)?.to_vec())
    }
}

macro_rules! wire_number {
    ($($ty:ty),*) => {$(
        impl Wire for $ty {
            const MIN_BYTES: usize = std::mem::size_of::<$ty>();
            fn put(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
                Ok(<$ty>::from_le_bytes(input.array()?))
            }
            fn put_slice(values: &[Self], out: &mut Vec<u8>) {
                let byte_len = values
                    .len()
                    .checked_mul(Self::MIN_BYTES)
                    .expect("GPU compute sequence exceeds capacity");
                out.reserve(byte_len);
                for value in values {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
            fn get_vec(input: &mut Reader<'_>, len: usize) -> Result<Vec<Self>, WireError> {
                let byte_len = len
                    .checked_mul(Self::MIN_BYTES)
                    .ok_or(WireError("sequence exceeds message"))?;
                let bytes = input.take(byte_len)?;
                Ok(bytes
                    .chunks_exact(Self::MIN_BYTES)
                    .map(|chunk| <$ty>::from_le_bytes(chunk.try_into().unwrap()))
                    .collect())
            }
        }
    )*};
}
wire_number!(u16, u32, i32, u64, f32);

impl Wire for bool {
    const MIN_BYTES: usize = 1;
    fn put(&self, out: &mut Vec<u8>) {
        out.push(u8::from(*self));
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::get(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError("invalid boolean")),
        }
    }
}

impl Wire for usize {
    const MIN_BYTES: usize = 8;
    fn put(&self, out: &mut Vec<u8>) {
        (*self as u64).put(out);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        usize::try_from(u64::get(input)?).map_err(|_| WireError("index overflow"))
    }
}

impl<T: Wire, const N: usize> Wire for [T; N] {
    const MIN_BYTES: usize = T::MIN_BYTES * N;
    fn put(&self, out: &mut Vec<u8>) {
        T::put_slice(self, out);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let mut error = None;
        let values: [Option<T>; N] = std::array::from_fn(|_| {
            if error.is_some() {
                return None;
            }
            match T::get(input) {
                Ok(value) => Some(value),
                Err(err) => {
                    error = Some(err);
                    None
                }
            }
        });
        if let Some(err) = error {
            return Err(err);
        }
        Ok(values.map(|value| value.unwrap()))
    }
}

impl<T: Wire> Wire for Box<T> {
    const MIN_BYTES: usize = T::MIN_BYTES;
    fn put(&self, out: &mut Vec<u8>) {
        (**self).put(out);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        T::get(input).map(Box::new)
    }
}

impl<T: Wire> Wire for Option<T> {
    const MIN_BYTES: usize = 1;
    fn put(&self, out: &mut Vec<u8>) {
        self.is_some().put(out);
        if let Some(value) = self {
            value.put(out);
        }
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(if bool::get(input)? {
            Some(T::get(input)?)
        } else {
            None
        })
    }
}

impl<T: Wire> Wire for Vec<T> {
    const MIN_BYTES: usize = 4;
    fn put(&self, out: &mut Vec<u8>) {
        put_sequence(self, out);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = u32::get(input)? as usize;
        if len.saturating_mul(T::MIN_BYTES.max(1)) > input.bytes.len() {
            return Err(WireError("sequence exceeds message"));
        }
        T::get_vec(input, len)
    }
}

impl Wire for String {
    const MIN_BYTES: usize = 4;
    fn put(&self, out: &mut Vec<u8>) {
        put_sequence(self.as_bytes(), out);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        String::from_utf8(Vec::<u8>::get(input)?).map_err(|_| WireError("invalid text"))
    }
}

impl<A: Wire, B: Wire, C: Wire> Wire for (A, B, C) {
    const MIN_BYTES: usize = A::MIN_BYTES + B::MIN_BYTES + C::MIN_BYTES;
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
        self.1.put(out);
        self.2.put(out);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok((A::get(input)?, B::get(input)?, C::get(input)?))
    }
}

impl<A: Wire, B: Wire, C: Wire, D: Wire> Wire for (A, B, C, D) {
    const MIN_BYTES: usize = A::MIN_BYTES + B::MIN_BYTES + C::MIN_BYTES + D::MIN_BYTES;
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
        self.1.put(out);
        self.2.put(out);
        self.3.put(out);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok((
            A::get(input)?,
            B::get(input)?,
            C::get(input)?,
            D::get(input)?,
        ))
    }
}

macro_rules! wire_struct {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        impl Wire for $ty {
            const MIN_BYTES: usize = 0;
            fn put(&self, out: &mut Vec<u8>) {
                let $ty { $($field),* } = self;
                $($field.put(out);)*
            }
            fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
                Ok($ty { $($field: Wire::get(input)?),* })
            }
        }
    };
}

wire_struct!(QuickDrawGpuMask {
    origin,
    size,
    coverage,
    threshold,
    repeat_transfer
});
wire_struct!(QuickDrawGpuCommand {
    mask,
    clip_rows,
    polygon_edges,
    area,
    rect,
    border,
    pattern,
    transfer
});
impl Wire for QuickDrawGpuPixel {
    const MIN_BYTES: usize = 5;

    fn put(&self, out: &mut Vec<u8>) {
        let Self { value, covered } = self;
        value.put(out);
        covered.put(out);
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            value: Wire::get(input)?,
            covered: Wire::get(input)?,
        })
    }

    fn put_slice(values: &[Self], out: &mut Vec<u8>) {
        let byte_len = values
            .len()
            .checked_mul(Self::MIN_BYTES)
            .expect("GPU compute sequence exceeds capacity");
        out.reserve(byte_len);
        for Self { value, covered } in values {
            out.extend_from_slice(&value.to_le_bytes());
            out.push(u8::from(*covered));
        }
    }

    fn get_vec(input: &mut Reader<'_>, len: usize) -> Result<Vec<Self>, WireError> {
        let byte_len = len
            .checked_mul(Self::MIN_BYTES)
            .ok_or(WireError("sequence exceeds message"))?;
        let bytes = input.take(byte_len)?;
        let mut pixels = Vec::with_capacity(len);
        for chunk in bytes.chunks_exact(Self::MIN_BYTES) {
            let covered = match chunk[4] {
                0 => false,
                1 => true,
                _ => return Err(WireError("invalid boolean")),
            };
            pixels.push(Self {
                value: u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                covered,
            });
        }
        Ok(pixels)
    }
}
wire_struct!(QuickDrawGpuDirectColor {
    source_depth,
    destination_depth,
    palette
});
wire_struct!(QuickDrawGpuBlitClip {
    coverage,
    transparent
});
wire_struct!(QuickDrawGpuArithmetic {
    depth,
    mode,
    op,
    source_clut,
    destination_clut,
    background
});
wire_struct!(QuickDrawGpuDeepMask {
    source_depth,
    destination_depth,
    mask_depth,
    mode,
    classic_colors,
    source_clut,
    destination_clut,
    mask_clut,
    mask_width,
    mask_height,
    mask_pixels,
    mask_columns,
    mask_rows,
    background
});
wire_struct!(QuickDrawGpuBlit {
    width,
    height,
    pixels,
    columns,
    rows,
    palette,
    direct,
    clip,
    arithmetic,
    deep_mask
});
wire_struct!(QuickDrawGpuPackedCopy {
    depth,
    width,
    height,
    source_stride,
    source_bit,
    source,
    destination_stride,
    destination_bit,
    background,
    palette
});
wire_struct!(PpcQ3GpuTexture {
    width,
    height,
    rgba,
    wrap_u,
    wrap_v
});
wire_struct!(PpcQ3LightData {
    is_on,
    brightness,
    color
});
wire_struct!(PpcQ3LightRecord {
    light,
    light_type,
    data,
    kind
});
wire_struct!(PpcQ3RasterLighting {
    lights,
    interpolation_style,
    illumination_type
});
wire_struct!(PpcQ3FogStyleData {
    state,
    mode,
    fog_start,
    fog_end,
    density,
    color
});
wire_struct!(PpcQ3RasterShading {
    highlight,
    normal,
    world,
    view_direction,
    specular_color,
    specular_control
});
wire_struct!(PpcQ3RasterVertex {
    shading,
    ambient_coefficient,
    fog_depth,
    vertex_alpha,
    screen_x,
    screen_y,
    depth,
    reciprocal_w,
    uv,
    color
});
wire_struct!(PpcQ3RasterDraw {
    primitive,
    test_depth,
    lighting,
    ambient_light,
    ambient_per_vertex,
    fog,
    texture,
    vertices,
    blend,
    write_depth
});
wire_struct!(PpcQ3RasterFrame {
    width,
    height,
    viewport,
    clear_color,
    textures,
    draws
});

impl Wire for PpcQ3RasterPrimitive {
    const MIN_BYTES: usize = 1;
    fn put(&self, out: &mut Vec<u8>) {
        out.push(*self as u8);
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::get(input)? {
            0 => Ok(Self::Triangles),
            1 => Ok(Self::Lines),
            2 => Ok(Self::Points),
            _ => Err(WireError("invalid primitive")),
        }
    }
}

impl Wire for PpcQ3LightKind {
    const MIN_BYTES: usize = 1;
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Ambient => out.push(0),
            Self::Directional {
                casts_shadows,
                direction,
            } => {
                out.push(1);
                casts_shadows.put(out);
                direction.put(out);
            }
            Self::Point {
                casts_shadows,
                attenuation,
                location,
            } => {
                out.push(2);
                casts_shadows.put(out);
                attenuation.put(out);
                location.put(out);
            }
            Self::Spot {
                casts_shadows,
                attenuation,
                location,
                direction,
                hot_angle,
                outer_angle,
                fall_off,
            } => {
                out.push(3);
                casts_shadows.put(out);
                attenuation.put(out);
                location.put(out);
                direction.put(out);
                hot_angle.put(out);
                outer_angle.put(out);
                fall_off.put(out);
            }
        }
    }
    fn get(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(input)? {
            0 => Self::Ambient,
            1 => Self::Directional {
                casts_shadows: Wire::get(input)?,
                direction: Wire::get(input)?,
            },
            2 => Self::Point {
                casts_shadows: Wire::get(input)?,
                attenuation: Wire::get(input)?,
                location: Wire::get(input)?,
            },
            3 => Self::Spot {
                casts_shadows: Wire::get(input)?,
                attenuation: Wire::get(input)?,
                location: Wire::get(input)?,
                direction: Wire::get(input)?,
                hot_angle: Wire::get(input)?,
                outer_angle: Wire::get(input)?,
                fall_off: Wire::get(input)?,
            },
            _ => return Err(WireError("invalid light kind")),
        })
    }
}

/// One synchronous operation. Each variant mirrors a renderer trait method.
#[derive(Debug, Clone)]
pub enum Request {
    Raster {
        command: QuickDrawGpuCommand,
        background: Vec<u32>,
    },
    BlitCells(QuickDrawGpuBlit),
    Blit(QuickDrawGpuBlit),
    PackedBlit(QuickDrawGpuPackedCopy),
    Qd3d {
        frame: PpcQ3RasterFrame,
        background: Vec<u16>,
        clut: Option<Box<[[u16; 3]; 256]>>,
    },
}

/// A declined operation. The guest falls back to the CPU for it; a fatal
/// failure (device loss, invalid state) keeps all later work on the CPU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub fatal: bool,
    pub message: String,
}

/// Successful results, committed to guest memory by the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Pixels(Vec<QuickDrawGpuPixel>),
    Words(Vec<u32>),
    Bytes(Vec<u8>),
    Halfwords(Vec<u16>),
}

/// Borrowed view of [`Request`] so the emulation worker can encode guest
/// snapshots directly without cloning textures, vertices or backgrounds.
#[derive(Clone, Copy)]
pub enum RequestRef<'a> {
    Raster {
        command: &'a QuickDrawGpuCommand,
        background: &'a [u32],
    },
    BlitCells(&'a QuickDrawGpuBlit),
    Blit(&'a QuickDrawGpuBlit),
    PackedBlit(&'a QuickDrawGpuPackedCopy),
    Qd3d {
        frame: &'a PpcQ3RasterFrame,
        background: &'a [u16],
        clut: Option<&'a [[u16; 3]; 256]>,
    },
}

impl<'a> RequestRef<'a> {
    pub fn kind(&self) -> RequestKind {
        match self {
            Self::Raster { .. } => RequestKind::Raster,
            Self::BlitCells(_) | Self::Blit(_) | Self::PackedBlit(_) => RequestKind::Blit,
            Self::Qd3d { .. } => RequestKind::Qd3d,
        }
    }

    fn encoded_capacity_hint(&self) -> usize {
        match self {
            Self::Raster {
                command,
                background,
            } => 64usize
                .saturating_add(
                    command
                        .mask
                        .as_ref()
                        .map_or(0, |mask| 24usize.saturating_add(mask.coverage.len())),
                )
                .saturating_add(command.polygon_edges.as_ref().map_or(0, |edges| {
                    4usize.saturating_add(edges.len().saturating_mul(16))
                }))
                .saturating_add(background.len().saturating_mul(4)),
            Self::BlitCells(command) | Self::Blit(command) => 64usize
                .saturating_add(command.pixels.len().saturating_mul(4))
                .saturating_add(
                    command
                        .columns
                        .len()
                        .saturating_add(command.rows.len())
                        .saturating_mul(8),
                ),
            Self::PackedBlit(command) => 64usize
                .saturating_add(command.source.len())
                .saturating_add(command.background.len())
                .saturating_add(usize::from(command.palette.is_some()) * 256),
            Self::Qd3d {
                frame,
                background,
                clut,
            } => {
                let textures = frame.textures.iter().fold(0usize, |acc, tex| {
                    acc.saturating_add(14).saturating_add(tex.rgba.len())
                });
                let draws = frame.draws.iter().fold(0usize, |acc, draw| {
                    let lights = draw.lighting.as_ref().map_or(0, |l| {
                        16usize.saturating_add(l.lights.len().saturating_mul(80))
                    });
                    acc.saturating_add(88)
                        .saturating_add(lights)
                        .saturating_add(draw.vertices.len().saturating_mul(108))
                });
                64usize
                    .saturating_add(textures)
                    .saturating_add(draws)
                    .saturating_add(background.len().saturating_mul(2))
                    .saturating_add(usize::from(clut.is_some()) * (256 * 6))
            }
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_capacity_hint());
        out.push(WIRE_VERSION);
        match *self {
            Self::Raster {
                command,
                background,
            } => {
                out.push(1);
                command.put(&mut out);
                put_sequence(background, &mut out);
            }
            Self::BlitCells(command) => {
                out.push(2);
                command.put(&mut out);
            }
            Self::Blit(command) => {
                out.push(3);
                command.put(&mut out);
            }
            Self::PackedBlit(command) => {
                out.push(4);
                command.put(&mut out);
            }
            Self::Qd3d {
                frame,
                background,
                clut,
            } => {
                out.push(5);
                frame.put(&mut out);
                put_sequence(background, &mut out);
                clut.is_some().put(&mut out);
                if let Some(clut) = clut {
                    clut.put(&mut out);
                }
            }
        }
        out
    }

    /// Upper bound of the encoded response, so the shared reply buffer can be
    /// sized before the emulation worker blocks.
    pub fn response_capacity(&self) -> usize {
        const HEADER: usize = 16;
        const ERROR_TEXT: usize = 4096;
        let payload = match *self {
            Self::Raster { command, .. } => command.pixel_count().unwrap_or(0).saturating_mul(5),
            Self::BlitCells(command) => blit_cells(command).saturating_mul(5),
            Self::Blit(command) => blit_cells(command).saturating_mul(4),
            Self::PackedBlit(command) => {
                (command.destination_stride as usize).saturating_mul(command.height as usize)
            }
            Self::Qd3d { frame, .. } => (frame.width as usize)
                .saturating_mul(frame.height as usize)
                .saturating_mul(2),
        };
        payload.max(ERROR_TEXT).saturating_add(HEADER)
    }
}

impl Request {
    #[cfg(test)]
    pub fn as_ref(&self) -> RequestRef<'_> {
        match self {
            Self::Raster {
                command,
                background,
            } => RequestRef::Raster {
                command,
                background,
            },
            Self::BlitCells(command) => RequestRef::BlitCells(command),
            Self::Blit(command) => RequestRef::Blit(command),
            Self::PackedBlit(command) => RequestRef::PackedBlit(command),
            Self::Qd3d {
                frame,
                background,
                clut,
            } => RequestRef::Qd3d {
                frame,
                background,
                clut: clut.as_deref(),
            },
        }
    }

    #[cfg(test)]
    pub fn encode(&self) -> Vec<u8> {
        self.as_ref().encode()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut input = Reader::new(bytes);
        if u8::get(&mut input)? != WIRE_VERSION {
            return Err(WireError("version mismatch"));
        }
        let request = match u8::get(&mut input)? {
            1 => Self::Raster {
                command: Wire::get(&mut input)?,
                background: Wire::get(&mut input)?,
            },
            2 => Self::BlitCells(Wire::get(&mut input)?),
            3 => Self::Blit(Wire::get(&mut input)?),
            4 => Self::PackedBlit(Wire::get(&mut input)?),
            5 => Self::Qd3d {
                frame: Wire::get(&mut input)?,
                background: Wire::get(&mut input)?,
                clut: Wire::get(&mut input)?,
            },
            _ => return Err(WireError("unknown request")),
        };
        input.finish()?;
        Ok(request)
    }

    /// Upper bound of the encoded response, so the shared reply buffer can be
    /// sized before the emulation worker blocks.
    #[cfg(test)]
    pub fn response_capacity(&self) -> usize {
        self.as_ref().response_capacity()
    }
}

fn blit_cells(command: &QuickDrawGpuBlit) -> usize {
    command.columns.len().saturating_mul(command.rows.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestKind {
    Raster,
    Blit,
    Qd3d,
}

pub fn encode_response(result: &Result<Response, Failure>) -> Vec<u8> {
    match result {
        Ok(Response::Pixels(pixels)) => {
            let mut out = Vec::with_capacity(6usize.saturating_add(pixels.len().saturating_mul(5)));
            out.extend_from_slice(&[WIRE_VERSION, 1]);
            pixels.put(&mut out);
            out
        }
        Ok(Response::Words(words)) => {
            let mut out = Vec::with_capacity(6usize.saturating_add(words.len().saturating_mul(4)));
            out.extend_from_slice(&[WIRE_VERSION, 2]);
            words.put(&mut out);
            out
        }
        Ok(Response::Bytes(bytes)) => {
            let mut out = Vec::with_capacity(6usize.saturating_add(bytes.len()));
            out.extend_from_slice(&[WIRE_VERSION, 3]);
            bytes.put(&mut out);
            out
        }
        Ok(Response::Halfwords(values)) => {
            let mut out = Vec::with_capacity(6usize.saturating_add(values.len().saturating_mul(2)));
            out.extend_from_slice(&[WIRE_VERSION, 4]);
            values.put(&mut out);
            out
        }
        Err(failure) => {
            // Keep failures within the reply capacity reserved for text.
            let mut end = failure.message.len().min(2048);
            while !failure.message.is_char_boundary(end) {
                end -= 1;
            }
            let text = &failure.message[..end];
            let mut out = Vec::with_capacity(6usize.saturating_add(text.len()));
            out.extend_from_slice(&[WIRE_VERSION, if failure.fatal { 5 } else { 0 }]);
            put_sequence(text.as_bytes(), &mut out);
            out
        }
    }
}

pub fn decode_response(bytes: &[u8]) -> Result<Response, Failure> {
    let mut input = Reader::new(bytes);
    let decoded = (|| {
        if u8::get(&mut input)? != WIRE_VERSION {
            return Err(WireError("version mismatch"));
        }
        Ok(match u8::get(&mut input)? {
            status @ (0 | 5) => Err(Failure {
                fatal: status == 5,
                message: String::get(&mut input)?,
            }),
            1 => Ok(Response::Pixels(Wire::get(&mut input)?)),
            2 => Ok(Response::Words(Wire::get(&mut input)?)),
            3 => Ok(Response::Bytes(Wire::get(&mut input)?)),
            4 => Ok(Response::Halfwords(Wire::get(&mut input)?)),
            _ => return Err(WireError("unknown response")),
        })
    })();
    let malformed = |error: WireError| Failure {
        fatal: true,
        message: error.to_string(),
    };
    match decoded {
        Ok(result) => {
            input.finish().map_err(malformed)?;
            result
        }
        Err(error) => Err(malformed(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(x: f32, y: f32) -> PpcQ3RasterVertex {
        PpcQ3RasterVertex {
            shading: PpcQ3RasterShading {
                highlight: true,
                normal: Some([0., 0., 1.]),
                world: [1., 2., 3.],
                view_direction: None,
                specular_color: [0.5, 0.25, 0.125],
                specular_control: 8.,
            },
            ambient_coefficient: 0.75,
            fog_depth: 3.5,
            vertex_alpha: Some(0.5),
            screen_x: x,
            screen_y: y,
            depth: 0.25,
            reciprocal_w: 2.,
            uv: [0.5, -1.5],
            color: [1., 0.5, 0.25, 1.],
        }
    }

    fn frame() -> PpcQ3RasterFrame {
        let light = |kind| PpcQ3LightRecord {
            light: 7,
            light_type: 9,
            data: PpcQ3LightData {
                is_on: 1,
                brightness: 0.8,
                color: (1., 0.5, 0.),
            },
            kind,
        };
        PpcQ3RasterFrame {
            width: 64,
            height: 48,
            viewport: [1, 2, 60, 40],
            clear_color: Some([0.1, 0.2, 0.3, 1.]),
            textures: vec![PpcQ3GpuTexture {
                width: 1,
                height: 2,
                rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
                wrap_u: true,
                wrap_v: false,
            }],
            draws: vec![PpcQ3RasterDraw {
                primitive: PpcQ3RasterPrimitive::Lines,
                test_depth: false,
                lighting: Some(PpcQ3RasterLighting {
                    lights: vec![
                        light(PpcQ3LightKind::Ambient),
                        light(PpcQ3LightKind::Directional {
                            casts_shadows: 1,
                            direction: (0., -1., 0.),
                        }),
                        light(PpcQ3LightKind::Point {
                            casts_shadows: 0,
                            attenuation: 2,
                            location: (1., 2., 3.),
                        }),
                        light(PpcQ3LightKind::Spot {
                            casts_shadows: 0,
                            attenuation: 1,
                            location: (4., 5., 6.),
                            direction: (0., 0., -1.),
                            hot_angle: 0.3,
                            outer_angle: 0.6,
                            fall_off: 3,
                        }),
                    ],
                    interpolation_style: 1,
                    illumination_type: 2,
                }),
                ambient_light: Some([0.2, 0.2, 0.2]),
                ambient_per_vertex: true,
                fog: Some(PpcQ3FogStyleData {
                    state: 1,
                    mode: 2,
                    fog_start: 0.1,
                    fog_end: 0.9,
                    density: 0.5,
                    color: (0.5, 0.5, 0.5, 1.),
                }),
                texture: Some(0),
                vertices: vec![vertex(1., 2.), vertex(3., 4.), vertex(5., 6.)],
                blend: true,
                write_depth: false,
            }],
        }
    }

    fn blit() -> QuickDrawGpuBlit {
        QuickDrawGpuBlit {
            width: 3,
            height: 2,
            pixels: vec![1, 2, 3, 4, 5, 6],
            columns: vec![[0, 1], [1, 3]],
            rows: vec![[0, 2]],
            palette: Some(std::array::from_fn(|i| i as u32 * 3)),
            direct: Some(QuickDrawGpuDirectColor {
                source_depth: 16,
                destination_depth: 8,
                palette: vec![0x7fff; 256],
            }),
            clip: Some(QuickDrawGpuBlitClip {
                coverage: Some(vec![1, 0]),
                transparent: Some([5, 0xffffff]),
            }),
            arithmetic: Some(QuickDrawGpuArithmetic {
                depth: 8,
                mode: 34,
                op: [1, 2, 3],
                source_clut: vec![[1, 2, 3]],
                destination_clut: vec![[4, 5, 6]],
                background: vec![9, 8],
            }),
            deep_mask: Some(QuickDrawGpuDeepMask {
                source_depth: 8,
                destination_depth: 16,
                mask_depth: 1,
                mode: 3,
                classic_colors: true,
                source_clut: vec![[1, 1, 1]],
                destination_clut: vec![],
                mask_clut: vec![[0, 0, 0], [65535, 65535, 65535]],
                mask_width: 2,
                mask_height: 1,
                mask_pixels: vec![0, 1],
                mask_columns: vec![0, 1],
                mask_rows: vec![0],
                background: vec![7, 7],
            }),
        }
    }

    fn assert_round_trip(request: Request) {
        let bytes = request.encode();
        let decoded = Request::decode(&bytes).unwrap();
        // Each request type implements Debug but not PartialEq; equal
        // re-encodings prove every field survived.
        assert_eq!(decoded.encode(), bytes);
        assert_eq!(format!("{decoded:?}"), format!("{request:?}"));
    }

    #[test]
    fn every_request_round_trips_all_fields() {
        assert_round_trip(Request::Raster {
            command: QuickDrawGpuCommand {
                mask: Some(QuickDrawGpuMask {
                    origin: [-3, 4],
                    size: [2, 1],
                    coverage: vec![1, 2],
                    threshold: 1,
                    repeat_transfer: true,
                }),
                clip_rows: Some(vec![vec![0, 2], vec![]]),
                polygon_edges: Some(vec![[0., 2., 1.5, -0.25]]),
                area: [0, 0, 2, 2],
                rect: [-1, -1, 3, 3],
                border: Some([1, 2]),
                pattern: [0xaa, 0x55, 0, 0xff, 1, 2, 3, 4],
                transfer: [[1, 2], [3, 4]],
            },
            background: vec![u32::MAX, 0, 7, 8],
        });
        assert_round_trip(Request::BlitCells(blit()));
        assert_round_trip(Request::Blit(blit()));
        assert_round_trip(Request::PackedBlit(QuickDrawGpuPackedCopy {
            depth: 4,
            width: 3,
            height: 2,
            source_stride: 2,
            source_bit: 4,
            source: vec![0x12, 0x34, 0x56, 0x78],
            destination_stride: 2,
            destination_bit: 0,
            background: vec![9, 9, 9, 9],
            palette: Some(std::array::from_fn(|i| 255 - i as u8)),
        }));
        assert_round_trip(Request::Qd3d {
            frame: frame(),
            background: vec![0x8000; 64 * 48],
            clut: Some(Box::new(std::array::from_fn(|i| [i as u16; 3]))),
        });
    }

    #[test]
    fn responses_round_trip_and_errors_stay_within_text_capacity() {
        for response in [
            Response::Pixels(vec![
                QuickDrawGpuPixel {
                    value: 9,
                    covered: true,
                },
                QuickDrawGpuPixel {
                    value: 0,
                    covered: false,
                },
            ]),
            Response::Words(vec![1, u32::MAX]),
            Response::Bytes(vec![0, 255]),
            Response::Halfwords(vec![0x8000, 0x7fff]),
        ] {
            let bytes = encode_response(&Ok(response.clone()));
            assert_eq!(decode_response(&bytes), Ok(response));
        }
        let long = "é".repeat(5000);
        let bytes = encode_response(&Err(Failure {
            fatal: false,
            message: long,
        }));
        assert!(bytes.len() <= 2048 + 16);
        let failure = decode_response(&bytes).unwrap_err();
        assert!(!failure.fatal && failure.message.len() <= 2048);
        let lost = Failure {
            fatal: true,
            message: "lost".into(),
        };
        assert_eq!(
            decode_response(&encode_response(&Err(lost.clone()))),
            Err(lost)
        );
    }

    #[test]
    fn malformed_messages_are_rejected_without_large_allocations() {
        assert!(Request::decode(&[]).is_err());
        assert!(Request::decode(&[WIRE_VERSION + 1, 1]).is_err());
        assert!(Request::decode(&[WIRE_VERSION, 99]).is_err());
        // A sequence that claims four billion words in a short message.
        let mut packet = vec![WIRE_VERSION, 3];
        packet.extend([0; 8]);
        packet.extend(u32::MAX.to_le_bytes());
        assert_eq!(
            Request::decode(&packet).unwrap_err(),
            WireError("sequence exceeds message")
        );
        let mut trailing = Request::Blit(blit()).encode();
        trailing.push(0);
        assert_eq!(
            Request::decode(&trailing).unwrap_err(),
            WireError("trailing bytes")
        );
        assert!(
            decode_response(&[WIRE_VERSION, 2, 1, 0, 0, 0])
                .unwrap_err()
                .fatal
        );
    }

    #[test]
    fn response_capacity_bounds_each_successful_reply() {
        let request = Request::Qd3d {
            frame: frame(),
            background: vec![0; 64 * 48],
            clut: None,
        };
        let reply = encode_response(&Ok(Response::Halfwords(vec![0; 64 * 48])));
        assert!(reply.len() <= request.response_capacity());
        let request = Request::BlitCells(blit());
        let reply = encode_response(&Ok(Response::Pixels(vec![
            QuickDrawGpuPixel {
                value: 0,
                covered: true
            };
            2
        ])));
        assert!(reply.len() <= request.response_capacity());
    }

    #[test]
    fn borrowed_requests_and_bulk_sequences_match_scalar_wire_format() {
        let mut rich_frame = frame();
        rich_frame.textures[0].rgba = (0..517).map(|i| (i * 37 + 11) as u8).collect();
        for v in &mut rich_frame.draws[0].vertices {
            v.shading.view_direction = Some([-0.25, 0.5, 0.75]);
        }
        let background: Vec<u16> = (0..(64 * 48))
            .map(|i| ((i as u16).wrapping_mul(0x1357)) ^ 0x8001)
            .collect();
        let clut: [[u16; 3]; 256] = std::array::from_fn(|i| {
            [
                (i as u16) * 257,
                ((255 - i) as u16) * 257,
                ((i * 7) as u16) ^ 0x55aa,
            ]
        });

        let borrowed = RequestRef::Qd3d {
            frame: &rich_frame,
            background: &background,
            clut: Some(&clut),
        };
        let hint = borrowed.encoded_capacity_hint();
        let borrowed_capacity = borrowed.response_capacity();
        let encoded = borrowed.encode();
        assert!(
            encoded.len() <= hint,
            "Qd3d capacity hint {hint} underestimated {} encoded bytes",
            encoded.len()
        );

        // Compare against an independent element-by-element scalar reference encoding.
        let mut expected = vec![WIRE_VERSION, 5];
        rich_frame.width.put(&mut expected);
        rich_frame.height.put(&mut expected);
        for v in rich_frame.viewport {
            v.put(&mut expected);
        }
        rich_frame.clear_color.is_some().put(&mut expected);
        for c in rich_frame.clear_color.unwrap() {
            c.put(&mut expected);
        }
        (rich_frame.textures.len() as u32).put(&mut expected);
        for tex in &rich_frame.textures {
            tex.width.put(&mut expected);
            tex.height.put(&mut expected);
            (tex.rgba.len() as u32).put(&mut expected);
            for &b in &tex.rgba {
                expected.push(b);
            }
            tex.wrap_u.put(&mut expected);
            tex.wrap_v.put(&mut expected);
        }
        rich_frame.draws.put(&mut expected);
        (background.len() as u32).put(&mut expected);
        for &hw in &background {
            expected.extend_from_slice(&hw.to_le_bytes());
        }
        true.put(&mut expected);
        for rgb in &clut {
            for &ch in rgb {
                expected.extend_from_slice(&ch.to_le_bytes());
            }
        }
        assert_eq!(encoded, expected);

        let owned = Request::Qd3d {
            frame: rich_frame,
            background: background.clone(),
            clut: Some(Box::new(clut)),
        };
        assert_eq!(owned.encode(), encoded);
        assert_eq!(owned.response_capacity(), borrowed_capacity);
        let Request::Qd3d {
            frame: decoded_frame,
            background: decoded_bg,
            clut: decoded_clut,
        } = Request::decode(&encoded).unwrap()
        else {
            panic!("expected Qd3d");
        };
        assert_eq!(decoded_bg, background);
        assert_eq!(*decoded_clut.unwrap(), clut);
        assert_eq!(decoded_frame.textures[0].rgba.len(), 517);

        // Exact single-allocation sizing and validation on response slices.
        let halfword_reply = encode_response(&Ok(Response::Halfwords(background.clone())));
        assert_eq!(halfword_reply.len(), 6 + background.len() * 2);
        assert_eq!(halfword_reply.capacity(), halfword_reply.len());
        assert_eq!(
            decode_response(&halfword_reply),
            Ok(Response::Halfwords(background))
        );

        let mut bad_pixels = encode_response(&Ok(Response::Pixels(vec![QuickDrawGpuPixel {
            value: 0x1122_3344,
            covered: true,
        }])));
        *bad_pixels.last_mut().unwrap() = 2;
        assert!(decode_response(&bad_pixels).unwrap_err().fatal);

        let mut short_array = Reader::new(&[1, 0, 0, 0, 2, 0]);
        assert_eq!(
            <[u32; 2]>::get(&mut short_array),
            Err(WireError("truncated"))
        );
    }
}
