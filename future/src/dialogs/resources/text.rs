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
                        matches!(item.kind & 0x7f, 8 | 16)
                            && u32::from_be_bytes(
                                raw[item.offset..item.offset + 4].try_into().unwrap(),
                            ) == handle
                    })
                    .ok_or_else(|| bad("dialog text item was replaced outside the manager"))?;
                if m.read(GuestPtr(dialog + 110), 1)?[0] != 0 && g.is_none() {
                    return Err(bad("visible dialog text replacement requires a GPU"));
                }
                owner = Some((dialog, item.rect, item.kind & 0x7f == 16, record.editor));
                break;
            }
            let active_editor = if let Some((_, _, true, editor)) =
                owner.filter(|(_, _, _, editor)| *editor != 0)
            {
                let mut te = std::mem::take(&mut p.textedit);
                let result = te.call(m, p, crate::textedit::Op::GetText(editor), 0);
                p.textedit = te;
                (result? == handle).then_some(editor)
            } else {
                None
            };
            if let Some(editor) = active_editor {
                let mut te = std::mem::take(&mut p.textedit);
                let result = te.call(
                    m,
                    p,
                    crate::textedit::Op::SetText {
                        source: GuestPtr(
                            string
                                .0
                                .checked_add(1)
                                .ok_or_else(|| bad("dialog text address overflow"))?,
                        ),
                        length: bytes.len() as u32,
                        handle: editor,
                    },
                    0,
                );
                p.textedit = te;
                result?;
            } else {
                p.replace_handles(m, vec![(handle, bytes.clone())])?;
            }
            if p.mem_error != 0 {
                return Ok(());
            }
            if let Some((dialog, rect, edit, _)) = owner {
                if let Some(editor) = active_editor {
                    if m.read(GuestPtr(dialog + 110), 1)?[0] != 0 {
                        let gpu = g.ok_or_else(|| bad("visible editText requires a GPU"))?;
                        let mut te = std::mem::take(&mut p.textedit);
                        let result = te.redraw_dialog(m, p, gpu, editor);
                        p.textedit = te;
                        result?;
                    }
                } else {
                    self.redraw_text(m, p, g, dialog, rect, (&bytes, !edit))?;
                }
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
