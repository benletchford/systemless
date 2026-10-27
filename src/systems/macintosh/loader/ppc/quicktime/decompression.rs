//! Image Compression Manager image descriptions and persistent codec state.
//!
//! The guest entry points are added alongside the destination/sequence plumbing;
//! these routines never substitute a generated frame for undecodable input.
use super::super::{
    ppc_arithmetic_transfer_channel, ppc_pixmap_pixel_rgb, ppc_point_in_region_storage,
    ppc_port_op_color_from_graf_vars, ppc_read_ctable_clut, ppc_read_pixmap_handle_bits,
    ppc_read_pixmap_raw_pixel, ppc_read_rgb_color, ppc_rgb_to_pixmap_pixel,
    ppc_write_pixmap_raw_pixel, PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET, PPC_RGB_BLACK, PPC_RGB_WHITE,
};
use super::*;

const BAD_DATA: i16 = -8969; // codecBadDataErr, Universal Interfaces MacErrors.h.
const NO_CODEC: i16 = -8961; // noCodecErr.
const DESCRIPTION_SIZE: usize = 86;
const MAX_DATA: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Description {
    pub codec: u32,
    pub width: usize,
    pub height: usize,
    pub data_size: usize,
    pub depth: u16,
    pub clut_id: i16,
    pub custom_clut: Option<Vec<u8>>,
}

impl Description {
    /// Packed ImageDescription, including a possible inline ColorTable.
    /// Validate idSize against the actual handle rather than accessible heap bytes.
    pub(crate) fn read(
        memory: &mut PpcSectionMem,
        handles: &[PpcHandleRecord],
        handle: u32,
    ) -> Result<Self, i16> {
        let record = handles
            .iter()
            .find(|record| record.handle == handle)
            .ok_or(PPC_PARAM_ERR)?;
        let data = memory
            .read_u32_be(handle)
            .filter(|data| *data != 0)
            .ok_or(PPC_PARAM_ERR)?;
        let size = memory.read_u32_be(data).ok_or(PPC_PARAM_ERR)? as usize;
        if !(DESCRIPTION_SIZE..=MAX_DATA).contains(&size) || size > record.size as usize {
            return Err(BAD_DATA);
        }
        let mut bytes = vec![0; size];
        memory
            .read_bytes_into(data, &mut bytes)
            .ok_or(PPC_PARAM_ERR)?;
        Self::parse(&bytes)
    }

    fn color_table_bytes(
        &self,
        resources: &[PpcVfsResourceRecord],
        current_refnum: i16,
    ) -> Result<Vec<u8>, i16> {
        if let Some(table) = &self.custom_clut {
            return Ok(table.clone());
        }
        let id = if self.clut_id > 0 {
            self.clut_id as u16
        } else {
            self.depth
        };
        let colors = if matches!(id, 1 | 2 | 4 | 8) {
            TrapDispatcher::standard_mac_indexed_clut(id)
                .map(|(colors, count)| colors[..count].to_vec())
        } else if matches!(id, 33 | 34 | 36 | 40) {
            let count = 1usize << (id - 32);
            Some(
                (0..count)
                    .map(|i| [((count - 1 - i) * 65535 / (count - 1)) as u16; 3])
                    .collect(),
            )
        } else {
            None
        };
        if let Some(colors) = colors {
            let mut table = Vec::with_capacity(8 + colors.len() * 8);
            table.extend_from_slice(&(-1i32).to_be_bytes());
            table.extend_from_slice(&0u16.to_be_bytes());
            table.extend_from_slice(&((colors.len() - 1) as u16).to_be_bytes());
            for (i, color) in colors.iter().enumerate() {
                table.extend_from_slice(&(i as u16).to_be_bytes());
                for component in color {
                    table.extend_from_slice(&component.to_be_bytes());
                }
            }
            return Ok(table);
        }
        if self.clut_id > 0 {
            let index = super::super::ppc_vfs_resource_index(
                resources,
                current_refnum,
                u32::from_be_bytes(*b"clut"),
                self.clut_id,
                false,
            )
            .ok_or(PPC_RES_NOT_FOUND_ERR)?;
            let data = &resources[index].data;
            if data.len() < 8 {
                return Err(BAD_DATA);
            }
            let count = i32::from(i16::from_be_bytes([data[6], data[7]])) + 1;
            if !(0..=256).contains(&count) {
                return Err(BAD_DATA);
            }
            let mut table = data.get(..8 + count as usize * 8).ok_or(BAD_DATA)?.to_vec();
            table[..4].copy_from_slice(&(-1i32).to_be_bytes());
            return Ok(table);
        }
        // Inside Macintosh: QuickTime (1993), 3-88: a noncustom table is
        // identified by CTSize == 0 or a negative CTSeed, not a nil Handle.
        let mut table = vec![0; 16];
        table[..4].copy_from_slice(&(-1i32).to_be_bytes());
        Ok(table)
    }

    fn resolve_raw_palette(
        &mut self,
        resources: &[PpcVfsResourceRecord],
        current_refnum: i16,
    ) -> Result<(), i16> {
        if self.codec == u32::from_be_bytes(*b"raw ")
            && matches!(self.depth, 1 | 2 | 4 | 8 | 33 | 34 | 36 | 40)
        {
            self.custom_clut = Some(self.color_table_bytes(resources, current_refnum)?);
        }
        Ok(())
    }

    fn parse(bytes: &[u8]) -> Result<Self, i16> {
        let u16_at = |offset: usize| -> Result<u16, i16> {
            Ok(u16::from_be_bytes(
                bytes
                    .get(offset..offset + 2)
                    .ok_or(BAD_DATA)?
                    .try_into()
                    .unwrap(),
            ))
        };
        let u32_at = |offset: usize| -> Result<u32, i16> {
            Ok(u32::from_be_bytes(
                bytes
                    .get(offset..offset + 4)
                    .ok_or(BAD_DATA)?
                    .try_into()
                    .unwrap(),
            ))
        };
        let size = u32_at(0)? as usize;
        if !(DESCRIPTION_SIZE..=MAX_DATA).contains(&size) || size > bytes.len() {
            return Err(BAD_DATA);
        }
        let width = usize::from(u16_at(32)?);
        let height = usize::from(u16_at(34)?);
        let data_size = u32_at(44)? as usize;
        if width == 0 || height == 0 || width > 4096 || height > 4096 || data_size > MAX_DATA {
            return Err(BAD_DATA);
        }
        let clut_id = u16_at(84)? as i16;
        let custom_clut = if clut_id == 0 {
            let count = i32::from(u16_at(92)? as i16) + 1;
            if !(0..=256).contains(&count) {
                return Err(BAD_DATA);
            }
            let end = DESCRIPTION_SIZE + 8 + count as usize * 8;
            if end > size {
                return Err(BAD_DATA);
            }
            Some(bytes[DESCRIPTION_SIZE..end].to_vec())
        } else {
            None
        };
        Ok(Self {
            codec: u32_at(4)?,
            width,
            height,
            data_size,
            depth: u16_at(82)?,
            clut_id,
            custom_clut,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Decoder {
    Cinepak(PpcQuickTimeCinepakDecoder),
    Still(u32),
    Raw,
}

impl Decoder {
    pub(crate) fn new(description: &Description) -> Result<Self, i16> {
        if description.codec == u32::from_be_bytes(*b"raw ") {
            raw_layout(description)?;
            return Ok(Self::Raw);
        }
        if description.codec == u32::from_be_bytes(*b"cvid") {
            return PpcQuickTimeCinepakDecoder::new(description.width, description.height)
                .map(Self::Cinepak)
                .ok_or(BAD_DATA);
        }
        if ppc_qt_graphics_image_format(description.codec).is_some() {
            return Ok(Self::Still(description.codec));
        }
        Err(NO_CODEC)
    }

    pub(crate) fn decode(
        &mut self,
        description: &Description,
        data: &[u8],
    ) -> Result<PpcQtGraphicsImage, i16> {
        if data.len() > MAX_DATA {
            return Err(BAD_DATA);
        }
        let image = match self {
            Self::Cinepak(decoder) => {
                // A malformed delta must not corrupt the reference image/codebooks
                // used by the next frame in this sequence.
                let mut next = decoder.clone();
                next.decode_sample(data).ok_or(BAD_DATA)?;
                let mut rgba = Vec::with_capacity(next.width * next.height * 4);
                for pixel in next.frame.chunks_exact(3) {
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
                let image = PpcQtGraphicsImage {
                    width: next.width as u32,
                    height: next.height as u32,
                    rgba,
                };
                *decoder = next;
                image
            }
            Self::Still(codec) => ppc_qt_decode_graphics_image(*codec, data).ok_or(BAD_DATA)?,
            Self::Raw => decode_raw(description, data)?,
        };
        if image.width as usize != description.width || image.height as usize != description.height
        {
            return Err(BAD_DATA);
        }
        Ok(image)
    }
}

fn raw_layout(description: &Description) -> Result<(usize, usize), i16> {
    let depth = match description.depth {
        33 | 34 | 36 | 40 => description.depth - 32,
        depth => depth,
    };
    if !matches!(depth, 1 | 2 | 4 | 8 | 16 | 24 | 32) {
        return Err(NO_CODEC);
    }
    // QuickTime File Format, Video sample data: every raw scanline starts
    // on an even-byte boundary, including packed RGB24 and indexed pixels.
    let row = ((description.width * depth as usize).div_ceil(8) + 1) & !1;
    let size = row
        .checked_mul(description.height)
        .filter(|size| *size <= MAX_DATA)
        .ok_or(BAD_DATA)?;
    Ok((row, size))
}

fn decode_raw(description: &Description, data: &[u8]) -> Result<PpcQtGraphicsImage, i16> {
    let (row, size) = raw_layout(description)?;
    if data.len() < size {
        return Err(BAD_DATA);
    }
    let depth = if description.depth > 32 {
        description.depth - 32
    } else {
        description.depth
    };
    let mut palette = [None; 256];
    if depth <= 8 {
        let table = description.color_table_bytes(&[], 0)?;
        let flags = u16::from_be_bytes([table[4], table[5]]);
        for (i, entry) in table[8..].chunks_exact(8).enumerate() {
            let index = if flags & 0x8000 != 0 {
                i
            } else {
                u16::from_be_bytes([entry[0], entry[1]]) as usize
            };
            if index >= palette.len() {
                return Err(BAD_DATA);
            }
            palette[index] = Some([entry[2], entry[4], entry[6], 255]);
        }
    }
    let mut rgba = Vec::with_capacity(description.width * description.height * 4);
    for y in 0..description.height {
        let row = &data[y * row..(y + 1) * row];
        for x in 0..description.width {
            let pixel = match depth {
                1 | 2 | 4 | 8 => {
                    let bit = x * depth as usize;
                    let index = usize::from(
                        (row[bit / 8] >> (8 - depth - (bit % 8) as u16))
                            & ((1u16 << depth) - 1) as u8,
                    );
                    palette[index].ok_or(BAD_DATA)?
                }
                16 => {
                    let word = u16::from_be_bytes([row[x * 2], row[x * 2 + 1]]);
                    let rgb = super::super::ppc_rgb555_to_rgb16(word);
                    [
                        (rgb[0] >> 8) as u8,
                        (rgb[1] >> 8) as u8,
                        (rgb[2] >> 8) as u8,
                        255,
                    ]
                }
                24 => [row[x * 3], row[x * 3 + 1], row[x * 3 + 2], 255],
                32 => [row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3], row[x * 4]],
                _ => unreachable!(),
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    Ok(PpcQtGraphicsImage {
        width: description.width as u32,
        height: description.height as u32,
        rgba,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Transform(pub [i32; 9]);

impl Default for Transform {
    fn default() -> Self {
        Self([65536, 0, 0, 0, 65536, 0, 0, 0, 1 << 30])
    }
}

impl Transform {
    pub(crate) fn read(memory: &mut PpcSectionMem, address: u32) -> Result<Self, i16> {
        if address == 0 {
            return Ok(Self::default());
        }
        let mut values = [0; 9];
        for (i, value) in values.iter_mut().enumerate() {
            *value = memory
                .read_u32_be(address.checked_add(i as u32 * 4).ok_or(PPC_PARAM_ERR)?)
                .ok_or(PPC_PARAM_ERR)? as i32;
        }
        let result = Self(values);
        result.inverse()?;
        Ok(result)
    }

    // QuickTime stores row-vector [a,b,u,c,d,v,tx,ty,w]. The perspective
    // column uses 2.30, while the remaining terms use signed 16.16.
    pub(crate) fn inverse(&self) -> Result<[f64; 9], i16> {
        let m = self.0;
        let a = [
            m[0] as f64 / 65536.0,
            m[3] as f64 / 65536.0,
            m[6] as f64 / 65536.0,
            m[1] as f64 / 65536.0,
            m[4] as f64 / 65536.0,
            m[7] as f64 / 65536.0,
            m[2] as f64 / 1073741824.0,
            m[5] as f64 / 1073741824.0,
            m[8] as f64 / 1073741824.0,
        ];
        let cofactors = [
            a[4] * a[8] - a[5] * a[7],
            a[2] * a[7] - a[1] * a[8],
            a[1] * a[5] - a[2] * a[4],
            a[5] * a[6] - a[3] * a[8],
            a[0] * a[8] - a[2] * a[6],
            a[2] * a[3] - a[0] * a[5],
            a[3] * a[7] - a[4] * a[6],
            a[1] * a[6] - a[0] * a[7],
            a[0] * a[4] - a[1] * a[3],
        ];
        let determinant = a[0] * cofactors[0] + a[1] * cofactors[3] + a[2] * cofactors[6];
        if determinant == 0.0 || !determinant.is_finite() {
            return Err(PPC_PARAM_ERR);
        }
        Ok(cofactors.map(|v| v / determinant))
    }

    pub(crate) fn source_point(inverse: &[f64; 9], x: f64, y: f64) -> Option<(f64, f64)> {
        let w = inverse[6] * x + inverse[7] * y + inverse[8];
        if w == 0.0 {
            return None;
        }
        let sx = (inverse[0] * x + inverse[1] * y + inverse[2]) / w;
        let sy = (inverse[3] * x + inverse[4] * y + inverse[5]) / w;
        (sx.is_finite() && sy.is_finite()).then_some((sx, sy))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sequence {
    pub description: Description,
    pub decoder: Decoder,
    pub port: u32,
    pub source: (i16, i16, i16, i16),
    pub transform: Transform,
    pub mode: u16,
    pub mask: u32,
    pub frame: Option<PpcQtGraphicsImage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sequences {
    next_id: u32,
    records: std::collections::BTreeMap<u32, Sequence>,
}

impl Default for Sequences {
    fn default() -> Self {
        Self {
            next_id: 1,
            records: Default::default(),
        }
    }
}

impl Sequences {
    pub(crate) fn insert(&mut self, sequence: Sequence) -> Result<u32, i16> {
        if self.next_id > i32::MAX as u32 {
            return Err(PPC_MEM_FULL_ERR);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.records.insert(id, sequence);
        Ok(id)
    }
    pub(crate) fn get_mut(&mut self, id: u32) -> Result<&mut Sequence, i16> {
        self.records.get_mut(&id).ok_or(PPC_PARAM_ERR)
    }
    pub(crate) fn end(&mut self, id: u32) -> Result<(), i16> {
        self.records.remove(&id).map(|_| ()).ok_or(PPC_PARAM_ERR)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn get_color_table(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap: &mut u32,
    mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    resources: &[PpcVfsResourceRecord],
    current_refnum: i16,
) -> i16 {
    let result = (|| {
        let out = cpu.gpr[4];
        if out == 0 || !ppc_memory_can_write_bytes(memory, out, 4) {
            return Err(PPC_PARAM_ERR);
        }
        let description = Description::read(memory, handles, cpu.gpr[3])?;
        let data = description.color_table_bytes(resources, current_refnum)?;
        let handle =
            ppc_process_alloc_handle_with_bytes(manager, memory, heap, mem_error, handles, &data);
        if handle == 0 {
            return Err(PPC_MEM_FULL_ERR);
        }
        memory.write_u32_be(out, handle).ok_or(PPC_PARAM_ERR)?;
        Ok(())
    })();
    result.err().unwrap_or(PPC_NO_ERR)
}

pub(crate) fn begin(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    gworlds: &[PpcGWorldRecord],
    current_port: u32,
    sequences: &mut Sequences,
    resources: &[PpcVfsResourceRecord],
    current_refnum: i16,
) -> Result<(), i16> {
    let out = cpu.gpr[3];
    if out == 0 || !ppc_memory_can_write_bytes(memory, out, 4) {
        return Err(PPC_PARAM_ERR);
    }
    let argument = |memory: &mut PpcSectionMem, index: u32| -> Result<u32, i16> {
        if index < 8 {
            return Ok(cpu.gpr[3 + index as usize]);
        }
        memory
            .read_u32_be(
                cpu.gpr[1]
                    .checked_add(24 + index * 4)
                    .ok_or(PPC_PARAM_ERR)?,
            )
            .ok_or(PPC_PARAM_ERR)
    };
    let flags = argument(memory, 8)? as u16;
    let _accuracy = argument(memory, 9)?;
    let codec = argument(memory, 10)?;
    // Component selection wildcards choose the installed software decoder.
    if !matches!(codec, 0 | 0xffff_ffff | 0xffff_fffe | 0xffff_fffd) {
        return Err(NO_CODEC);
    }
    if flags & !(1 | 2 | 32) != 0 {
        return Err(-8962);
    }
    let mut description = Description::read(memory, handles, cpu.gpr[4])?;
    description.resolve_raw_palette(resources, current_refnum)?;
    let decoder = Decoder::new(&description)?;
    let port = if cpu.gpr[5] == 0 {
        current_port
    } else {
        cpu.gpr[5]
    };
    ppc_live_quickdraw_surface(memory, gworlds, port).ok_or(PPC_PARAM_ERR)?;
    let source = if cpu.gpr[7] == 0 {
        (0, 0, description.height as i16, description.width as i16)
    } else {
        ppc_read_rect(memory, cpu.gpr[7]).ok_or(PPC_PARAM_ERR)?
    };
    if source.0 < 0
        || source.1 < 0
        || source.2 <= source.0
        || source.3 <= source.1
        || source.2 as usize > description.height
        || source.3 as usize > description.width
    {
        return Err(PPC_PARAM_ERR);
    }
    let transform = Transform::read(memory, cpu.gpr[8])?;
    let mode = cpu.gpr[9] as u16;
    if !matches!(mode & !64,0..=7 | 32..=39) {
        return Err(-8962);
    }
    if cpu.gpr[10] != 0 {
        ppc_region_storage(memory, cpu.gpr[10]).ok_or(PPC_PARAM_ERR)?;
    }
    let id = sequences.insert(Sequence {
        description,
        decoder,
        port,
        source,
        transform,
        mode,
        mask: cpu.gpr[10],
        frame: None,
    })?;
    memory.write_u32_be(out, id).ok_or(PPC_PARAM_ERR)?;
    Ok(())
}

pub(crate) fn frame(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    sequences: &mut Sequences,
) -> Result<u16, i16> {
    use super::super::PpcPixMapBits;
    let flags = cpu.gpr[5] as u16;
    if flags & !(8 | 32 | 256 | 512 | 1024) != 0 {
        return Err(-8962);
    }
    let out = cpu.gpr[6];
    if out != 0 && !ppc_memory_can_write_bytes(memory, out, 2) {
        return Err(PPC_PARAM_ERR);
    }
    let sequence = sequences.get_mut(cpu.gpr[3])?;
    let first = sequence.frame.is_none();
    if flags & 1024 != 0 && first {
        return Err(-8965); // codecImageBufErr: no existing image buffer.
    }
    if flags & 32 != 0 && matches!(sequence.decoder, Decoder::Cinepak(_)) {
        return Err(-8968); // codecWouldOffscreenErr.
    }
    if flags & 256 == 0 {
        if cpu.gpr[4] == 0 {
            return Err(PPC_PARAM_ERR);
        }
        let size = if sequence.description.codec == u32::from_be_bytes(*b"cvid") {
            let mut header = [0; 4];
            memory
                .read_bytes_into(cpu.gpr[4], &mut header)
                .ok_or(PPC_PARAM_ERR)?;
            usize::from(header[1]) * 65536 + usize::from(header[2]) * 256 + usize::from(header[3])
        } else if matches!(sequence.decoder, Decoder::Raw) {
            raw_layout(&sequence.description)?.1
        } else {
            sequence.description.data_size
        };
        if size == 0 || size > MAX_DATA {
            return Err(BAD_DATA);
        }
        let mut data = vec![0; size];
        memory
            .read_bytes_into(cpu.gpr[4], &mut data)
            .ok_or(PPC_PARAM_ERR)?;
        sequence.frame = Some(sequence.decoder.decode(&sequence.description, &data)?);
    }
    let image = sequence.frame.as_ref().ok_or(BAD_DATA)?;
    if flags & 8 == 0 {
        let surface =
            ppc_live_quickdraw_surface(memory, gworlds, sequence.port).ok_or(PPC_PARAM_ERR)?;
        let fb = surface.front_buffer;
        if !matches!(fb.depth, 1 | 2 | 4 | 8 | 16 | 32) {
            return Err(PPC_PARAM_ERR);
        }
        let bottom = i16::try_from(i64::from(surface.top) + i64::from(fb.height))
            .map_err(|_| PPC_PARAM_ERR)?;
        let right = i16::try_from(i64::from(surface.left) + i64::from(fb.width))
            .map_err(|_| PPC_PARAM_ERR)?;
        let bits = PpcPixMapBits {
            base_addr: fb.base_addr,
            row_bytes: fb.row_bytes,
            top: surface.top,
            left: surface.left,
            bottom,
            right,
            width: fb.width,
            height: fb.height,
            depth: fb.depth,
        };
        let clut = if matches!(fb.depth, 2 | 4 | 8) {
            Some(
                ppc_read_ctable_clut(
                    memory,
                    surface.ctable_handle.ok_or(PPC_PARAM_ERR)?,
                    &[[0; 3]; 256],
                )
                .ok_or(PPC_PARAM_ERR)?,
            )
        } else {
            None
        };
        let mut masks = Vec::new();
        if sequence.mask != 0 {
            masks.push(ppc_region_storage(memory, sequence.mask).ok_or(PPC_PARAM_ERR)?);
        }
        if let Some(clip) = ppc_port_clip_storage(memory, sequence.port, None) {
            masks.push(clip);
        }
        if let Some(vis) = memory
            .read_u32_be(sequence.port.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
            .and_then(|handle| ppc_region_storage(memory, handle))
        {
            masks.push(vis);
        }
        draw_image(
            memory,
            image,
            bits,
            sequence.source,
            (bits.top, bits.left, bits.bottom, bits.right),
            sequence.mode,
            &masks,
            clut,
            [32768; 3],
            [65535; 3],
            Some(&sequence.transform.inverse()?),
        )?;
    }
    let result = 0x8000 | if first { 0x4000 } else { 0 };
    if out != 0 {
        memory.write_u16_be(out, result).ok_or(PPC_PARAM_ERR)?;
    }
    Ok(result)
}

/// Decode a single image into a caller-owned PixMap. Rectangles use that
/// PixMap's coordinate system, not the current port's origin.
pub(crate) fn decompress_image(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    current_port: u32,
    resources: &[PpcVfsResourceRecord],
    current_refnum: i16,
) -> i16 {
    decompress_image_inner(
        cpu,
        memory,
        handles,
        current_port,
        resources,
        current_refnum,
    )
    .map_or_else(|error| error, |_| PPC_NO_ERR)
}

fn decompress_image_inner(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    current_port: u32,
    resources: &[PpcVfsResourceRecord],
    current_refnum: i16,
) -> Result<(), i16> {
    let mut description = Description::read(memory, handles, cpu.gpr[4])?;
    description.resolve_raw_palette(resources, current_refnum)?;
    let mut decoder = Decoder::new(&description)?;
    let size = if matches!(decoder, Decoder::Raw) {
        raw_layout(&description)?.1
    } else {
        description.data_size
    };
    if cpu.gpr[3] == 0 || size == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let mut data = vec![0; size];
    memory
        .read_bytes_into(cpu.gpr[3], &mut data)
        .ok_or(PPC_PARAM_ERR)?;
    let bits = ppc_read_pixmap_handle_bits(memory, cpu.gpr[5]).ok_or(PPC_PARAM_ERR)?;
    if !matches!(bits.depth, 1 | 2 | 4 | 8 | 16 | 32) {
        return Err(PPC_PARAM_ERR);
    }
    let src = if cpu.gpr[6] == 0 {
        (0, 0, description.height as i16, description.width as i16)
    } else {
        ppc_read_rect(memory, cpu.gpr[6]).ok_or(PPC_PARAM_ERR)?
    };
    let dst = ppc_read_rect(memory, cpu.gpr[7]).ok_or(PPC_PARAM_ERR)?;
    let sw = i32::from(src.3) - i32::from(src.1);
    let sh = i32::from(src.2) - i32::from(src.0);
    let dw = i32::from(dst.3) - i32::from(dst.1);
    let dh = i32::from(dst.2) - i32::from(dst.0);
    if sw <= 0
        || sh <= 0
        || dw <= 0
        || dh <= 0
        || src.0 < 0
        || src.1 < 0
        || src.2 as usize > description.height
        || src.3 as usize > description.width
    {
        return Err(PPC_PARAM_ERR);
    }
    let mode = cpu.gpr[8] as u16;
    if !matches!(mode & !64, 0..=7 | 32..=39) {
        return Err(-8962); // codecUnimpErr; never silently substitute srcCopy.
    }
    let mask = if cpu.gpr[9] == 0 {
        None
    } else {
        Some(ppc_region_storage(memory, cpu.gpr[9]).ok_or(PPC_PARAM_ERR)?)
    };
    let pixmap = memory.read_u32_be(cpu.gpr[5]).ok_or(PPC_PARAM_ERR)?;
    let clut = if matches!(bits.depth, 2 | 4 | 8) {
        let table = memory
            .read_u32_be(pixmap.checked_add(42).ok_or(PPC_PARAM_ERR)?)
            .ok_or(PPC_PARAM_ERR)?;
        Some(ppc_read_ctable_clut(memory, table, &[[0; 3]; 256]).ok_or(PPC_PARAM_ERR)?)
    } else {
        None
    };
    let op = ppc_port_op_color_from_graf_vars(memory, current_port).unwrap_or(PPC_RGB_BLACK);
    let op = [op.red, op.green, op.blue];
    let back = current_port
        .checked_add(PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET)
        .and_then(|address| ppc_read_rgb_color(memory, address))
        .unwrap_or(PPC_RGB_WHITE);
    let back = [back.red, back.green, back.blue];
    let image = decoder.decode(&description, &data)?;
    draw_image(
        memory,
        &image,
        bits,
        src,
        dst,
        mode,
        &mask.into_iter().collect::<Vec<_>>(),
        clut,
        op,
        back,
        None,
    )
}

/// Floyd–Steinberg diffusion in 16-bit RGB space. Errors are retained at
/// sixteenth-component precision; row padding is never a diffusion target.
struct ErrorDiffusion {
    current: Vec<[i64; 3]>,
    next: Vec<[i64; 3]>,
}
impl ErrorDiffusion {
    fn new(width: usize) -> Self {
        Self {
            current: vec![[0; 3]; width + 2],
            next: vec![[0; 3]; width + 2],
        }
    }
    fn next_row(&mut self) {
        std::mem::swap(&mut self.current, &mut self.next);
        self.next.fill([0; 3]);
    }
    fn quantize(
        &mut self,
        index: usize,
        bits: super::super::PpcPixMapBits,
        rgb: [u16; 3],
        clut: Option<&[[u16; 3]; 256]>,
    ) -> Result<u32, i16> {
        let adjusted = std::array::from_fn(|i| {
            (i64::from(rgb[i]) + self.current[index + 1][i] / 16).clamp(0, 65535) as u16
        });
        self.current[index + 1] = [0; 3];
        let pixel = ppc_rgb_to_pixmap_pixel(bits, adjusted, clut).ok_or(PPC_PARAM_ERR)?;
        let actual = ppc_pixmap_pixel_rgb(bits, pixel, clut).ok_or(PPC_PARAM_ERR)?;
        for i in 0..3 {
            let error = i64::from(adjusted[i]) - i64::from(actual[i]);
            self.current[index + 2][i] += error * 7;
            self.next[index][i] += error * 3;
            self.next[index + 1][i] += error * 5;
            self.next[index + 2][i] += error;
        }
        Ok(pixel)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_image(
    memory: &mut PpcSectionMem,
    image: &PpcQtGraphicsImage,
    bits: super::super::PpcPixMapBits,
    src: (i16, i16, i16, i16),
    dst: (i16, i16, i16, i16),
    mode: u16,
    masks: &[Vec<u8>],
    clut: Option<[[u16; 3]; 256]>,
    op: [u16; 3],
    back: [u16; 3],
    inverse: Option<&[f64; 9]>,
) -> Result<(), i16> {
    let mut diffusion =
        (mode & 64 != 0 && bits.depth < 32).then(|| ErrorDiffusion::new(bits.width as usize));
    let mode = mode & !64;
    let sw = i32::from(src.3) - i32::from(src.1);
    let sh = i32::from(src.2) - i32::from(src.0);
    let dw = i32::from(dst.3) - i32::from(dst.1);
    let dh = i32::from(dst.2) - i32::from(dst.0);
    let pixel_mask = if bits.depth == 32 {
        0x00ff_ffff
    } else {
        (1u32 << bits.depth) - 1
    };
    for y in i32::from(dst.0.max(bits.top))..i32::from(dst.2.min(bits.bottom)) {
        for x in i32::from(dst.1.max(bits.left))..i32::from(dst.3.min(bits.right)) {
            if masks
                .iter()
                .any(|region| !ppc_point_in_region_storage(region, y as i16, x as i16))
            {
                continue;
            }
            let (sx, sy) = if let Some(inverse) = inverse {
                let Some((sx, sy)) =
                    Transform::source_point(inverse, x as f64 + 0.5, y as f64 + 0.5)
                else {
                    continue;
                };
                (sx.floor() as i64, sy.floor() as i64)
            } else {
                (
                    i64::from(src.1)
                        + i64::from(x - i32::from(dst.1)) * i64::from(sw) / i64::from(dw),
                    i64::from(src.0)
                        + i64::from(y - i32::from(dst.0)) * i64::from(sh) / i64::from(dh),
                )
            };
            if sx < i64::from(src.1)
                || sx >= i64::from(src.3)
                || sy < i64::from(src.0)
                || sy >= i64::from(src.2)
            {
                continue;
            }
            if sx < 0 || sy < 0 || sx >= image.width as i64 || sy >= image.height as i64 {
                continue;
            }
            let offset = (sy as usize * image.width as usize + sx as usize) * 4;
            let rgb = [
                image.rgba[offset],
                image.rgba[offset + 1],
                image.rgba[offset + 2],
            ]
            .map(|v| u16::from(v) * 257);
            if mode == 36 && rgb == back {
                continue;
            }
            let source = if mode <= 7 || mode == 36 {
                if let Some(diffusion) = diffusion.as_mut() {
                    diffusion.quantize(
                        (x - i32::from(bits.left)) as usize,
                        bits,
                        rgb,
                        clut.as_ref(),
                    )?
                } else {
                    ppc_rgb_to_pixmap_pixel(bits, rgb, clut.as_ref()).ok_or(PPC_PARAM_ERR)?
                }
            } else {
                0
            };
            let destination = ppc_read_pixmap_raw_pixel(memory, bits, x, y).ok_or(PPC_PARAM_ERR)?;
            let pixel = match mode {
                0 | 36 => source,
                1 => source | destination,
                2 => source ^ destination,
                3 => !source & destination,
                4 => !source,
                5 => !source | destination,
                6 => !source ^ destination,
                7 => source & destination,
                32..=39 => {
                    let old = ppc_pixmap_pixel_rgb(bits, destination, clut.as_ref())
                        .ok_or(PPC_PARAM_ERR)?;
                    let result = std::array::from_fn(|i| {
                        if mode == 32 {
                            ((u64::from(rgb[i]) * u64::from(op[i])
                                + u64::from(old[i]) * u64::from(65535 - op[i]))
                                / 65535) as u16
                        } else {
                            ppc_arithmetic_transfer_channel(mode, rgb[i], old[i], op[i])
                        }
                    });
                    if let Some(diffusion) = diffusion.as_mut() {
                        diffusion.quantize(
                            (x - i32::from(bits.left)) as usize,
                            bits,
                            result,
                            clut.as_ref(),
                        )?
                    } else {
                        ppc_rgb_to_pixmap_pixel(bits, result, clut.as_ref()).ok_or(PPC_PARAM_ERR)?
                    }
                }
                _ => unreachable!(),
            } & pixel_mask;
            ppc_write_pixmap_raw_pixel(memory, bits, x, y, pixel).ok_or(PPC_PARAM_ERR)?;
        }
        if let Some(diffusion) = diffusion.as_mut() {
            diffusion.next_row();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn description(codec: &[u8; 4], width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0; 86];
        bytes[..4].copy_from_slice(&86u32.to_be_bytes());
        bytes[4..8].copy_from_slice(codec);
        bytes[32..34].copy_from_slice(&width.to_be_bytes());
        bytes[34..36].copy_from_slice(&height.to_be_bytes());
        bytes[82..84].copy_from_slice(&24u16.to_be_bytes());
        bytes[84..86].copy_from_slice(&(-1i16).to_be_bytes());
        bytes
    }

    #[test]
    fn raw_rgb_rows_are_even_aligned_and_preserve_alpha() {
        let mut d = Description::parse(&description(b"raw ", 1, 2)).unwrap();
        let image = Decoder::new(&d)
            .unwrap()
            .decode(&d, &[255, 0, 0, 99, 0, 255, 0, 88])
            .unwrap();
        assert_eq!(image.rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(
            Decoder::new(&d)
                .unwrap()
                .decode(&d, &[255, 0, 0, 0, 255, 0]),
            Err(BAD_DATA)
        );
        d.depth = 32;
        assert_eq!(
            Decoder::new(&d)
                .unwrap()
                .decode(&d, &[17, 1, 2, 3, 255, 4, 5, 6])
                .unwrap()
                .rgba,
            [1, 2, 3, 17, 4, 5, 6, 255]
        );
        d.depth = 16;
        assert_eq!(
            Decoder::new(&d)
                .unwrap()
                .decode(&d, &[0x7c, 0, 0x03, 0xe0])
                .unwrap()
                .rgba,
            [255, 0, 0, 255, 0, 255, 0, 255]
        );
    }

    #[test]
    fn raw_indexed_pixels_use_custom_color_values_and_packed_rows() {
        let mut d = Description::parse(&description(b"raw ", 5, 1)).unwrap();
        d.depth = 1;
        d.custom_clut = Some(vec![
            0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 255, 255, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0,
        ]);
        let image = Decoder::new(&d).unwrap().decode(&d, &[0xa8, 0xee]).unwrap();
        assert_eq!(
            image.rgba,
            [0, 255, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255]
        );
    }

    #[test]
    fn transform_inverts_rotation_translation_and_perspective() {
        let rotate = Transform([0, 65536, 0, -65536, 0, 0, 10 * 65536, 20 * 65536, 1 << 30]);
        assert_eq!(
            Transform::source_point(&rotate.inverse().unwrap(), 7.0, 22.0),
            Some((2.0, 3.0))
        );
        let perspective = Transform([65536, 0, 1 << 28, 0, 65536, 0, 0, 0, 1 << 30]);
        let (x, y) =
            Transform::source_point(&perspective.inverse().unwrap(), 4.0 / 3.0, 0.0).unwrap();
        assert!((x - 2.0).abs() < 1e-10 && y == 0.0);
        assert_eq!(Transform([0; 9]).inverse(), Err(PPC_PARAM_ERR));
    }

    #[test]
    fn ending_one_sequence_preserves_other_and_does_not_reuse_id() {
        let d = Description::parse(&description(b"cvid", 4, 4)).unwrap();
        let sequence = Sequence {
            decoder: Decoder::new(&d).unwrap(),
            description: d,
            port: 42,
            source: (0, 0, 4, 4),
            transform: Transform::default(),
            mode: 0,
            mask: 0,
            frame: None,
        };
        let mut sequences = Sequences::default();
        let a = sequences.insert(sequence.clone()).unwrap();
        let b = sequences.insert(sequence.clone()).unwrap();
        sequences.end(a).unwrap();
        assert_eq!(sequences.end(a), Err(PPC_PARAM_ERR));
        assert_eq!(sequences.get_mut(b).unwrap().port, 42);
        assert!(sequences.insert(sequence).unwrap() > b);
    }

    #[test]
    fn description_uses_declared_size_and_owns_custom_table() {
        let mut bytes = description(b"cvid", 4, 4);
        bytes[84..86].copy_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&[
            0, 0, 0, 7, 0, 0, 0, 1, 0, 0, 255, 255, 0, 0, 0, 0, 0, 1, 0, 0, 255, 255, 0, 0,
        ]);
        assert_eq!(Description::parse(&bytes), Err(BAD_DATA));
        let size = bytes.len() as u32;
        bytes[..4].copy_from_slice(&size.to_be_bytes());
        let parsed = Description::parse(&bytes).unwrap();
        assert_eq!(parsed.custom_clut.as_ref().unwrap(), &bytes[86..]);
        bytes[100] = 19;
        assert_ne!(parsed.custom_clut.as_ref().unwrap(), &bytes[86..]);
        bytes.truncate(100);
        assert_eq!(Description::parse(&bytes), Err(BAD_DATA));
    }

    #[test]
    fn dimensions_and_unknown_codec_are_rejected() {
        assert_eq!(
            Description::parse(&description(b"cvid", 0, 4)),
            Err(BAD_DATA)
        );
        let d = Description::parse(&description(b"xxxx", 4, 4)).unwrap();
        assert_eq!(Decoder::new(&d), Err(NO_CODEC));
    }

    #[test]
    fn still_decoder_uses_pixels_and_checks_description_dimensions() {
        let image =
            image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 128, 255, 64]).unwrap();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let d = Description::parse(&description(b"png ", 2, 1)).unwrap();
        let mut decoder = Decoder::new(&d).unwrap();
        let decoded = decoder.decode(&d, bytes.get_ref()).unwrap();
        assert_eq!(decoded.rgba, [255, 0, 0, 255, 0, 128, 255, 64]);
        let wrong_size = Description::parse(&description(b"png ", 3, 1)).unwrap();
        assert_eq!(decoder.decode(&wrong_size, bytes.get_ref()), Err(BAD_DATA));
        assert_eq!(decoder.decode(&d, b"damaged PNG"), Err(BAD_DATA));
    }

    #[test]
    fn cinepak_sequences_keep_independent_references_after_malformed_delta() {
        let d = Description::parse(&description(b"cvid", 4, 4)).unwrap();
        let mut decoder = Decoder::new(&d).unwrap();
        let untouched = Decoder::new(&d).unwrap();
        // One 4x4 V1 block: four luma values and neutral chroma.
        let sample = [
            0, 0, 0, 37, 0, 4, 0, 4, 0, 1, 0x10, 0, 0, 27, 0, 0, 0, 0, 0, 4, 0, 4, 0x22, 0, 0, 10,
            0, 85, 170, 255, 0, 0, 0x32, 0, 0, 5, 0,
        ];
        let first = decoder.decode(&d, &sample).unwrap();
        assert_eq!(&first.rgba[..4], &[0, 0, 0, 255]);
        assert_eq!(&first.rgba[8..12], &[85, 85, 85, 255]);
        assert_ne!(decoder, untouched);
        let before = decoder.clone();
        let mut damaged = sample;
        damaged[26] = 255; // Change codebook before the invalid trailing chunk.
        damaged[35] = 6; // Chunk extends past the end of the strip.
        assert_eq!(decoder.decode(&d, &damaged), Err(BAD_DATA));
        assert_eq!(decoder, before);
        let skip = [
            0, 0, 0, 30, 0, 4, 0, 4, 0, 1, 0x11, 0, 0, 20, 0, 0, 0, 0, 0, 4, 0, 4, 0x31, 0, 0, 8,
            0, 0, 0, 0,
        ];
        assert_eq!(decoder.decode(&d, &skip).unwrap(), first);
        let mut other = untouched;
        assert_ne!(other.decode(&d, &skip).unwrap(), first);
    }
}
