//! Selection shares one TERec while each item retains its own text handle.
use super::*;
impl Dialogs {
    pub fn select_text(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: Option<&Graphics>,
        dialog: u32,
        number: i16,
        range: (i16, i16),
    ) -> Result<()> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            let record = self
                .records
                .get(&dialog)
                .filter(|r| !r.closed)
                .ok_or_else(|| bad("unknown or closed dialog"))?;
            if !p.windows.contains(m, dialog)?
                || m.u32(GuestPtr(dialog + 156))? != record.items
                || m.u32(GuestPtr(dialog + 160))? != record.editor
                || record.editor == 0
            {
                return Err(bad("dialog editText ownership changed"));
            }
            let raw = payload(m, p, record.items)?;
            let list = items(&raw)?;
            let index = number
                .checked_sub(1)
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&n| n < list.len())
                .ok_or_else(|| bad("invalid dialog editText item"))?;
            let item = &list[index];
            if item.kind & 0x7f != 16 {
                return Err(bad("dialog item is not editable text"));
            }
            let text = u32::from_be_bytes(raw[item.offset..item.offset + 4].try_into().unwrap());
            if !record.text.contains(&text) {
                return Err(bad("dialog editable text was replaced"));
            }
            let editor = record.editor;
            let visible = m.read(GuestPtr(dialog + 110), 1)?[0] != 0;
            let gpu = if visible {
                Some(g.ok_or_else(|| bad("visible editText selection requires a GPU"))?)
            } else {
                g
            };
            m.validate_write(GuestPtr(dialog + 164), 4)?;
            let ticks = m.u32(GuestPtr(0x16a))?;
            let mut te = std::mem::take(&mut p.textedit);
            let result = (|| {
                let current = te.call(m, p, crate::textedit::Op::GetText(editor), 0)?;
                let data = m.u32(GuestPtr(editor))?;
                if current != text {
                    if m.u16(GuestPtr(data + 36))? != 0 {
                        te.activate(
                            m,
                            p,
                            gpu.ok_or_else(|| bad("active editText requires GPU"))?,
                            editor,
                            false,
                            ticks,
                        )?;
                    }
                    // The copied DITL rectangle is guest-owned, canonical storage.
                    let copy = m.u32(GuestPtr(record.items))?;
                    te.bind_dialog_text(
                        m,
                        p,
                        editor,
                        text,
                        GuestPtr(copy + item.offset as u32 + 4),
                    )?;
                    if p.mem_error != 0 {
                        return Err(bad("dialog editText layout allocation failed"));
                    }
                }
                te.select(
                    m,
                    p,
                    gpu,
                    editor,
                    (range.0.max(0) as u32, range.1.max(0) as u32),
                    ticks,
                )?;
                let data = m.u32(GuestPtr(editor))?;
                if visible && m.u16(GuestPtr(data + 36))? == 0 {
                    te.activate(m, p, gpu.unwrap(), editor, true, ticks)?;
                }
                m.write(GuestPtr(dialog + 164), &(index as i16).to_be_bytes())?;
                m.write(GuestPtr(dialog + 166), &i16::from(visible).to_be_bytes())
            })();
            p.textedit = te;
            result
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
