//! Interrupt callback and asynchronous delivery tracking.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActiveInterruptCallbackSource {
    Adb,
    Timer,
    Vbl,
    CursorTask,
    SoundCallback,
    SoundFileCompletion,
    SoundDoubleBack,
    FileCompletion,
    DialogDrawProc,
    DialogFilterProc,
    MenuHook,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActiveInterruptCallback {
    pub(crate) source: ActiveInterruptCallbackSource,
    pub(crate) resume_pc: u32,
    pub(crate) resume_sp: u32,
    pub(crate) d_regs: [u32; 8],
    pub(crate) a_regs: [u32; 8],
    pub(crate) sr: u16,
    pub(crate) ccr: u8,
    pub(crate) restore_port: Option<(u32, u32)>,
}

pub(crate) fn is_sound_interrupt_source(source: ActiveInterruptCallbackSource) -> bool {
    matches!(
        source,
        ActiveInterruptCallbackSource::SoundCallback
            | ActiveInterruptCallbackSource::SoundFileCompletion
            | ActiveInterruptCallbackSource::SoundDoubleBack
    )
}

pub(crate) fn interrupt_callback_sr(source: ActiveInterruptCallbackSource, saved_sr: u16) -> u16 {
    match source {
        // A vertical retrace interrupt runs with processor priority level 1
        // and then restores the previous priority when it completes.
        // Inside Macintosh: Processes (1994), pp. 1-11 and 6-3.
        ActiveInterruptCallbackSource::Vbl | ActiveInterruptCallbackSource::CursorTask => {
            (saved_sr & !0x0700) | 0x2100
        }
        _ => saved_sr,
    }
}
