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
    };
    if !command.valid() {
        return None;
    }
    let output = if command.clip.is_some() {
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
        if source.depth == destination.depth
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
