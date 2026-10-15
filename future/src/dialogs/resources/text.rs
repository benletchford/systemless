//! Raw text-handle payloads and Pascal-string accessors.
use super::*;
impl Dialogs {
    pub fn item_text(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: Option<&Graphics>,
        handle: u32,
        string: GuestPtr,
        set: bool,
    ) -> Result<()> {
        let old_res = p.res_error;
        let old_mem = p.mem_error;
        let result = (|| {
            if string.0 == 0 {
                return Err(bad("nil dialog text string"));
            }
            if !set {
                let size = p.handle_size(m, handle)?;
                if p.mem_error != 0 {
                    return Err(bad("invalid dialog text handle"));
                }
                let n = size.min(255) as usize;
                let ptr = GuestPtr(m.u32(GuestPtr(handle))?);
                let bytes = m.read(ptr, n)?.to_vec();
                m.validate_write(string, n + 1)?;
                let mut out = vec![n as u8];
                out.extend(bytes);
                m.write(string, &out)?;
                return Ok(());
            }
            // Freeze Pascal input before a handle move, including aliases into its old data.
            let bytes = pstring(m, string)?[1..].to_vec();
            let mut owner = None;
            for (&dialog, record) in &self.records {
                if record.closed || !record.text.contains(&handle) {
                    continue;
                }
                if !p.windows.contains(m, dialog)?
                    || m.u32(GuestPtr(dialog + 156))? != record.items
                    || m.u16(GuestPtr(dialog + 108))? != 2
                {
                    return Err(bad("dialog text ownership changed"));
                }
                let raw = payload(m, p, record.items)?;
                let list = items(&raw)?;
                let item = list
                    .into_iter()
                    .find(|item| {
                        item.kind & 0x7f == 8
                            && u32::from_be_bytes(
                                raw[item.offset..item.offset + 4].try_into().unwrap(),
                            ) == handle
                    })
                    .ok_or_else(|| bad("dialog text item was replaced outside the manager"))?;
                if m.read(GuestPtr(dialog + 110), 1)?[0] != 0 && g.is_none() {
                    return Err(bad("visible dialog text replacement requires a GPU"));
                }
                owner = Some((dialog, item.rect));
                break;
            }
            p.replace_handles(m, vec![(handle, bytes.clone())])?;
            if p.mem_error != 0 {
                return Ok(());
            }
            if let Some((dialog, rect)) = owner {
                self.redraw_text(m, p, g, dialog, rect, &bytes)?;
            }
            Ok(())
        })();
        p.res_error = old_res;
        if !set {
            p.mem_error = old_mem;
        }
        result
    }
}
