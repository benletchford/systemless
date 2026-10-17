//! Native CopyBits sampling after adapter resolution and compatibility gates.
use super::*;

/// Returns only fully staged writes. A decline leaves the existing software
/// path responsible for sparse/unreadable source pixels and unsupported transfers.
pub(super) fn sample_copy(
    memory: &mut PpcSectionMem,
    source: PpcPixMapBits,
    destination: PpcPixMapBits,
    source_rect: [i32; 4],
    destination_rect: [i32; 4],
    clip: [i32; 4],
    palette: Option<[u32; 256]>,
    direct: Option<crate::quickdraw::gpu::QuickDrawGpuDirectColor>,
    operation_clip: Option<crate::quickdraw::gpu::QuickDrawGpuBlitClip>,
    mut arithmetic: Option<crate::quickdraw::gpu::QuickDrawGpuArithmetic>,
    mut deep_mask: Option<crate::quickdraw::gpu::QuickDrawGpuDeepMask>,
    details: &mut crate::memory::SavedPixels<()>,
) -> Option<Vec<(i32, i32, u32)>> {
    let renderer = memory.quickdraw_gpu_renderer();
    if !renderer.is_some() {
        return None;
    }
    let [st, sl, sb, sr] = source_rect;
    let [dt, dl, db, dr] = destination_rect;
    let [top, left, bottom, right] = clip;
    let xs: Vec<i32> = (left..right)
        .map(|x| {
            (i64::from(sl)
                + (i64::from(x) - i64::from(dl)) * i64::from(sr - sl) / i64::from(dr - dl))
                as i32
        })
        .collect();
    let ys: Vec<i32> = (top..bottom)
        .map(|y| {
            (i64::from(st)
                + (i64::from(y) - i64::from(dt)) * i64::from(sb - st) / i64::from(db - dt))
                as i32
        })
        .collect();
    let x0 = *xs.first()?;
    let x1 = xs.last()?.checked_add(1)?;
    let y0 = *ys.first()?;
    let y1 = ys.last()?.checked_add(1)?;
    if x0 < i32::from(source.left)
        || x1 > i32::from(source.right)
        || y0 < i32::from(source.top)
        || y1 > i32::from(source.bottom)
    {
        return None;
    }
    let width = u32::try_from(x1 - x0).ok()?;
    let height = u32::try_from(y1 - y0).ok()?;
    if u64::from(width) * u64::from(height) > 4_194_304
        || xs.len().checked_mul(ys.len())? > 4_194_304
    {
        return None;
    }
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in y0..y1 {
        for x in x0..x1 {
            pixels.push(ppc_read_pixmap_raw_pixel(memory, source, x, y)?);
        }
    }
    if let Some(a) = &mut arithmetic {
        a.background = Vec::with_capacity(xs.len() * ys.len());
        for y in top..bottom {
            for x in left..right {
                a.background
                    .push(ppc_read_pixmap_raw_pixel(memory, destination, x, y)?);
            }
        }
    }
    if let Some(d) = &mut deep_mask {
        d.background = Vec::with_capacity(xs.len() * ys.len());
        for y in top..bottom {
            for x in left..right {
                d.background
                    .push(ppc_read_pixmap_raw_pixel(memory, destination, x, y)?);
            }
        }
    }
    let command = crate::quickdraw::gpu::QuickDrawGpuBlit {
        width,
        height,
        pixels,
        columns: xs
            .iter()
            .map(|x| [(x - x0) as u32, (x - x0 + 1) as u32])
            .collect(),
        rows: ys
            .iter()
            .map(|y| [(y - y0) as u32, (y - y0 + 1) as u32])
            .collect(),
        palette,
        direct,
        clip: operation_clip,
        arithmetic,
        deep_mask,
    };
    if !command.valid() {
        return None;
    }
    let output =
        if command.clip.is_some() || command.arithmetic.is_some() || command.deep_mask.is_some() {
            renderer.render_blit_cells(&command).ok()?
        } else {
            renderer
                .render_blit(&command)
                .ok()?
                .into_iter()
                .map(|value| crate::quickdraw::gpu::QuickDrawGpuPixel {
                    value,
                    covered: true,
                })
                .collect()
        };
    let limit = if destination.depth == 32 {
        u32::MAX
    } else {
        (1 << destination.depth) - 1
    };
    if output.len() != xs.len() * ys.len() || output.iter().any(|p| p.value > limit) {
        return None;
    }
    let mut writes = Vec::with_capacity(output.len());
    for (i, cell) in output.into_iter().enumerate() {
        if !cell.covered {
            continue;
        }
        let pixel = cell.value;
        let row = i / xs.len();
        let col = i % xs.len();
        if command.deep_mask.is_none()
            && command.arithmetic.is_none()
            && source.depth == destination.depth
            && matches!(source.depth, 8 | 16)
            && command.pixels[((ys[row] - y0) as u32 * width + (xs[col] - x0) as u32) as usize]
                == pixel
        {
            let lanes = source.depth / 8;
            let address = source.base_addr
                + (ys[row] - i32::from(source.top)) as u32 * source.row_bytes
                + (xs[col] - i32::from(source.left)) as u32 * lanes;
            memory.presentation().capture_detail(
                details,
                writes.len() * lanes as usize,
                address,
                lanes as usize,
            );
        }
        writes.push((left + col as i32, top + row as i32, pixel));
    }
    Some(writes)
}

/// Snapshot a separately scaled raw mask, keeping palette expansion and blend
/// weights in the GPU kernel rather than baking destination colors on the host.
pub(super) fn deep_mask_operation(
    memory: &mut PpcSectionMem,
    source_depth: u32,
    destination_depth: u32,
    mask: PpcPixMapBits,
    source_clut: Option<&[[u16; 3]; 256]>,
    destination_clut: Option<&[[u16; 3]; 256]>,
    mask_clut: Option<&[[u16; 3]; 256]>,
    mode: u16,
    mask_rect: [i32; 4],
    destination_rect: [i32; 4],
    clip: [i32; 4],
) -> Option<crate::quickdraw::gpu::QuickDrawGpuDeepMask> {
    let [st, sl, sb, sr] = mask_rect;
    let [dt, dl, db, dr] = destination_rect;
    let [top, left, bottom, right] = clip;
    let xs: Vec<i32> = (left..right)
        .map(|x| {
            (i64::from(sl)
                + (i64::from(x) - i64::from(dl)) * i64::from(sr - sl) / i64::from(dr - dl))
                as i32
        })
        .collect();
    let ys: Vec<i32> = (top..bottom)
        .map(|y| {
            (i64::from(st)
                + (i64::from(y) - i64::from(dt)) * i64::from(sb - st) / i64::from(db - dt))
                as i32
        })
        .collect();
    let x0 = *xs.first()?;
    let x1 = xs.last()?.checked_add(1)?;
    let y0 = *ys.first()?;
    let y1 = ys.last()?.checked_add(1)?;
    if x0 < i32::from(mask.left)
        || x1 > i32::from(mask.right)
        || y0 < i32::from(mask.top)
        || y1 > i32::from(mask.bottom)
    {
        return None;
    }
    let width = u32::try_from(x1 - x0).ok()?;
    let height = u32::try_from(y1 - y0).ok()?;
    if u64::from(width) * u64::from(height) > 4_194_304
        || xs.len().checked_mul(ys.len())? > 4_194_304
    {
        return None;
    }
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in y0..y1 {
        for x in x0..x1 {
            pixels.push(ppc_read_pixmap_raw_pixel(memory, mask, x, y)?);
        }
    }
    let clut = |depth, colors: Option<&[[u16; 3]; 256]>| -> Option<Vec<[u16; 3]>> {
        if matches!(depth, 2 | 4 | 8) {
            Some(colors?[..1usize << depth].to_vec())
        } else {
            Some(Vec::new())
        }
    };
    Some(crate::quickdraw::gpu::QuickDrawGpuDeepMask {
        source_depth,
        destination_depth,
        mask_depth: mask.depth,
        mode: u32::from(mode),
        source_clut: clut(source_depth, source_clut)?,
        destination_clut: clut(destination_depth, destination_clut)?,
        mask_clut: clut(mask.depth, mask_clut)?,
        mask_width: width,
        mask_height: height,
        mask_pixels: pixels,
        mask_columns: xs.iter().map(|x| (x - x0) as u32).collect(),
        mask_rows: ys.iter().map(|y| (y - y0) as u32).collect(),
        background: Vec::new(),
    })
}
