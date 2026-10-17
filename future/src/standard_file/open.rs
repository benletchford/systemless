//! Classic SFGetFile filtering. TRUE excludes a file; folders always remain.
use super::*;
pub(super) struct Open {
    types: Option<Vec<[u8; 4]>>,
    filter: u32,
    pub source: Option<DirectoryView>,
    pending: Vec<u32>,
    next: usize,
    dirty: bool,
}
impl Dialog {
    pub(crate) fn start_open(
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        request: GetFile,
        ticks: u32,
    ) -> Result<Self> {
        request.validate(m)?;
        let types = if request.count == -1 {
            None
        } else {
            let bytes = if request.count == 0 {
                &[][..]
            } else {
                m.read(request.types, request.count as usize * 4)?
            };
            Some(bytes.as_chunks::<4>().0.to_vec())
        };
        // SFGetFile's prompt argument is ignored. Stable private Pascal strings
        // are snapshotted by start before it allocates anything.
        let errors = (p.mem_error, p.res_error);
        let temp_result = p.new_pointer(m, 16, true);
        (p.mem_error, p.res_error) = errors;
        let temp = temp_result?;
        if temp == 0 {
            return Err(bad("Standard File string allocation failed"));
        }
        m.write(GuestPtr(temp), b"\x09Open file")?;
        let result = Self::start_impl(
            m,
            p,
            g,
            PutFile {
                point: request.point,
                prompt: GuestPtr(temp),
                name: GuestPtr(temp + 15),
                hook: 0,
                reply: request.reply,
            },
            ticks,
            Some(Open {
                types,
                filter: request.filter,
                source: None,
                pending: Vec::new(),
                next: 0,
                dirty: true,
            }),
        );
        p.dispose_pointer(temp);
        (p.mem_error, p.res_error) = errors;
        result
    }
    /// Prepares one real guest invocation. Its result must be accepted before
    /// this method is called again. The runtime owns instruction budgeting.
    pub(crate) fn filter_call(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        ticks: u32,
    ) -> Result<Option<(u32, GuestPtr)>> {
        let Some(open) = &mut self.open else {
            return Ok(None);
        };
        let fresh = p
            .files
            .directory_view(self.directory.id)
            .map_err(|_| bad("Standard File directory disappeared"))?;
        if open.source.as_ref() != Some(&fresh) {
            open.pending = fresh
                .entries
                .iter()
                .filter(|e| {
                    !e.directory
                        && open.types.as_ref().is_none_or(|types| {
                            p.files.file_type(e.id).is_some_and(|t| types.contains(&t))
                        })
                })
                .map(|e| e.id)
                .collect();
            self.directory = fresh.clone();
            self.directory
                .entries
                .retain(|e| e.directory || open.pending.contains(&e.id));
            open.source = Some(fresh);
            open.next = 0;
            open.dirty = true;
            self.selected = None;
            self.page = 0;
        }
        if open.filter != 0 && open.next < open.pending.len() {
            let node = open.pending[open.next];
            let routine = open.filter;
            p.files
                .filter_info(m, node, self.slot(512), self.slot(640))?;
            return Ok(Some((routine, self.slot(512))));
        }
        if open.dirty {
            open.dirty = false;
            self.render(m, p, g, ticks)?;
        }
        Ok(None)
    }
    pub(crate) fn filtered(&mut self, excluded: bool) -> Result<()> {
        let open = self
            .open
            .as_mut()
            .ok_or_else(|| bad("unexpected Standard File filter return"))?;
        let node = *open
            .pending
            .get(open.next)
            .ok_or_else(|| bad("unexpected Standard File filter return"))?;
        open.next += 1;
        if excluded {
            self.directory.entries.retain(|e| e.id != node);
        }
        Ok(())
    }
}
