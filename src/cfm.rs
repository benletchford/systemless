//! Semantic resumption of one CFM load across guest initialization.
//! Inside Macintosh: PowerPC System Software (1994), pp. 3-15--3-18, 3-27.

/// Load identities use the connection reserved for that load. The connection
/// allocator never reuses these IDs, including when initialization fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmLoadId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmLoadOutputs {
    pub(crate) connection: u32,
    pub(crate) main_address: u32,
    pub(crate) error_name: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmLoadOperation {
    pub(crate) id: CfmLoadId,
    pub(crate) main_address: u32,
    pub(crate) outputs: CfmLoadOutputs,
    pub(crate) created_connection: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CfmLoadCompletion {
    Ready(CfmLoadOperation),
    InitializationFailed(CfmLoadId),
}

impl CfmLoadOperation {
    pub(crate) fn resume(self, initializer_result: u32) -> CfmLoadCompletion {
        if initializer_result == 0 {
            CfmLoadCompletion::Ready(self)
        } else {
            CfmLoadCompletion::InitializationFailed(self.id)
        }
    }
}
