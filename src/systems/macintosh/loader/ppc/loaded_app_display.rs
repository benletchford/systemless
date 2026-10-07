//! PowerPC loaded application front buffer inspection and window geometry methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    pub(crate) fn window_definition_id(&mut self, window: u32) -> i16 {
        ppc_window_proc_id(&mut self.memory, window)
    }
    pub fn current_front_buffer(&self) -> Option<PpcFrontBuffer> {
        ppc_front_buffer_for_gworld(&self.gworlds, *self.current_gworld)
    }

    pub fn presented_front_buffer(&self) -> Option<PpcFrontBuffer> {
        if self.draw_sprocket.active_context.is_some() || self.draw_sprocket.swap_count > 0 {
            ppc_front_buffer_for_gworld(&self.gworlds, self.draw_sprocket.front_buffer_gworld)
                .or_else(|| ppc_front_buffer_for_gworld(&self.gworlds, PPC_MAIN_GWORLD))
        } else {
            ppc_front_buffer_for_gworld(&self.gworlds, PPC_MAIN_GWORLD)
        }
    }

    pub fn read_front_buffer_row(
        &mut self,
        front_buffer: PpcFrontBuffer,
        row: u32,
        dst: &mut [u8],
    ) -> Option<()> {
        if row >= front_buffer.height || dst.len() < front_buffer.row_bytes as usize {
            return None;
        }
        let row_addr = front_buffer
            .base_addr
            .checked_add(row.checked_mul(front_buffer.row_bytes)?)?;
        self.memory
            .read_bytes_into(row_addr, &mut dst[..front_buffer.row_bytes as usize])?;
        Some(())
    }

    /// Number of live Window Manager records, excluding screen and offscreen
    /// graphics worlds.
    pub fn window_count(&mut self) -> usize {
        self.gworlds
            .iter()
            .filter(|record| {
                !matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
                    && self
                        .memory
                        .read_u16_be(record.port.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
                        .is_some_and(|kind| kind != 0)
            })
            .count()
    }

    /// Global content bounds of the frontmost visible Window Manager record.
    pub fn window_bounds(&mut self) -> (i16, i16, i16, i16) {
        ppc_front_visible_window(&mut self.memory, &self.gworlds)
            .and_then(|window| {
                ppc_window_global_content_bounds(&mut self.memory, &self.gworlds, window)
            })
            .unwrap_or((0, 0, 0, 0))
    }
}
