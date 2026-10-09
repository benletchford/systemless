//! Architecture-neutral Scrap Manager operations and parameter evaluation.
//!
//! Inside Macintosh Volume I (1985), chapter 15 "The Scrap Manager", pp. I-451--I-463;
//! Inside Macintosh: More Macintosh Toolbox (1993), chapter 2 "Scrap Manager", pp. 2-3--2-50.
//!
//! The Scrap Manager supports the Desk Scrap, which allows cutting, copying, and pasting
//! between documents and across applications and desk accessories.
//! The canonical evaluation functions below formalize the ABI contracts across 68k traps
//! ($A9F9 InfoScrap, $A9FA UnloadScrap, $A9FB LoadScrap, $A9FC ZeroScrap, $A9FD GetScrap, $A9FE PutScrap)
//! and PowerPC CFM imports.

/// Size of the standard Macintosh `ScrapStuff` record in bytes (Inside Macintosh I-457).
pub const SCRAP_STUFF_RECORD_SIZE: usize = 16;

/// Scrap Manager error: requested scrap flavor type was not found (`noTypeErr = -102`).
pub const NO_TYPE_ERR: i16 = -102;

/// Memory Manager error: not enough memory to resize destination handle (`memFullErr = -108`).
pub const MEM_FULL_ERR: i16 = -108;

/// Scrap Manager error: desk scrap is uninitialized (`noScrapErr = -100`).
pub const NO_SCRAP_ERR: i16 = -100;

/// General operating system parameter error (`paramErr = -50`).
pub const PARAM_ERR: i16 = -50;

/// Standard success result code (`noErr = 0`).
pub const NO_ERR: i16 = 0;

/// Evaluated fields for the 16-byte `ScrapStuff` record (Inside Macintosh I-457).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrapStuffRecord {
    pub scrap_size: u32,
    pub scrap_handle: u32,
    pub scrap_count: u16,
    pub scrap_state: i16,
    pub scrap_name: u32,
}

/// Evaluates fields for an `InfoScrap` response.
///
/// Per IM:I I-457: `FUNCTION InfoScrap: PScrapStuff;`
/// `scrapState` is positive (1) when the scrap is resident in memory, and 0 when on disk.
/// When resident in memory, `scrapHandle` points to the active desk-scrap handle.
#[inline]
pub fn evaluate_scrap_stuff_record(
    serialized_size: u32,
    scrap_handle: u32,
    count: u16,
    in_memory: bool,
) -> ScrapStuffRecord {
    ScrapStuffRecord {
        scrap_size: serialized_size,
        scrap_handle: if in_memory { scrap_handle } else { 0 },
        scrap_count: count,
        scrap_state: if in_memory { 1 } else { 0 },
        scrap_name: 0,
    }
}

/// Evaluates canonical parameters for `GetScrap`.
///
/// Per IM:I I-458: `FUNCTION GetScrap(hDest: Handle; theType: ResType; VAR offset: LONGINT): LONGINT;`
/// Reads data of the specified type from the desk scrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetScrapParameters {
    pub destination_handle: u32,
    pub flavor_type: [u8; 4],
    pub offset_ptr: u32,
}

impl GetScrapParameters {
    /// Returns `true` if `hDest` is NIL (0), indicating a query-only invocation.
    ///
    /// Per IM:I I-458: "If hDest is NIL, GetScrap returns the size and offset without copying data."
    #[allow(dead_code)]
    #[inline]
    pub const fn is_query_only(&self) -> bool {
        self.destination_handle == 0
    }
}

/// Evaluates canonical parameters for `GetScrap`.
#[inline]
pub fn evaluate_get_scrap_parameters(
    destination_handle: u32,
    flavor_type: [u8; 4],
    offset_ptr: u32,
) -> GetScrapParameters {
    GetScrapParameters {
        destination_handle,
        flavor_type,
        offset_ptr,
    }
}

/// Evaluates canonical parameters for `PutScrap`.
///
/// Per IM:I I-459: `FUNCTION PutScrap(length: LONGINT; theType: ResType; source: Ptr): LONGINT;`
/// Writes data of the specified type to the desk scrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PutScrapParameters {
    pub length: i32,
    pub flavor_type: [u8; 4],
    pub source_ptr: u32,
}

impl PutScrapParameters {
    /// Returns `true` if parameters represent a valid write payload.
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.length > 0 && self.source_ptr != 0
    }

    /// Payload data length in bytes.
    #[allow(dead_code)]
    #[inline]
    pub const fn data_length(&self) -> usize {
        if self.length > 0 {
            self.length as usize
        } else {
            0
        }
    }
}

/// Evaluates canonical parameters for `PutScrap`.
#[inline]
pub fn evaluate_put_scrap_parameters(
    length: i32,
    flavor_type: [u8; 4],
    source_ptr: u32,
) -> PutScrapParameters {
    PutScrapParameters {
        length,
        flavor_type,
        source_ptr,
    }
}

/// Evaluates invocation of `ZeroScrap`.
///
/// Per IM:I I-458: `FUNCTION ZeroScrap: LONGINT;`
/// Clears the desk scrap and increments the scrap change count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZeroScrapAction;

/// Evaluates canonical `ZeroScrap` invocation.
#[inline]
pub fn evaluate_zero_scrap() -> ZeroScrapAction {
    ZeroScrapAction
}

/// Evaluates invocation of `LoadScrap`.
///
/// Per IM:I I-458: `FUNCTION LoadScrap: LONGINT;`
/// Reads the desk scrap from disk into memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadScrapAction;

/// Evaluates canonical `LoadScrap` invocation.
#[inline]
pub fn evaluate_load_scrap() -> LoadScrapAction {
    LoadScrapAction
}

/// Evaluates invocation of `UnloadScrap`.
///
/// Per IM:I I-458: `FUNCTION UnloadScrap: LONGINT;`
/// Writes the desk scrap to disk and releases memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnloadScrapAction;

/// Evaluates canonical `UnloadScrap` invocation.
#[inline]
pub fn evaluate_unload_scrap() -> UnloadScrapAction {
    UnloadScrapAction
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrap_manager_evaluation() {
        // Constants
        assert_eq!(SCRAP_STUFF_RECORD_SIZE, 16);
        assert_eq!(NO_TYPE_ERR, -102);
        assert_eq!(MEM_FULL_ERR, -108);
        assert_eq!(NO_SCRAP_ERR, -100);
        assert_eq!(PARAM_ERR, -50);
        assert_eq!(NO_ERR, 0);

        // evaluate_scrap_stuff_record: resident in memory
        let in_mem = evaluate_scrap_stuff_record(42, 0x1234, 3, true);
        assert_eq!(in_mem.scrap_size, 42);
        assert_eq!(in_mem.scrap_handle, 0x1234);
        assert_eq!(in_mem.scrap_count, 3);
        assert_eq!(in_mem.scrap_state, 1);
        assert_eq!(in_mem.scrap_name, 0);

        // evaluate_scrap_stuff_record: on disk / unloaded
        let on_disk = evaluate_scrap_stuff_record(100, 0x1234, 4, false);
        assert_eq!(on_disk.scrap_size, 100);
        assert_eq!(on_disk.scrap_handle, 0);
        assert_eq!(on_disk.scrap_count, 4);
        assert_eq!(on_disk.scrap_state, 0);
        assert_eq!(on_disk.scrap_name, 0);

        // GetScrap
        let get_query = evaluate_get_scrap_parameters(0, *b"TEXT", 0x5000);
        assert_eq!(get_query.destination_handle, 0);
        assert_eq!(get_query.flavor_type, *b"TEXT");
        assert_eq!(get_query.offset_ptr, 0x5000);
        assert!(get_query.is_query_only());

        let get_copy = evaluate_get_scrap_parameters(0x6000, *b"PICT", 0x5004);
        assert_eq!(get_copy.destination_handle, 0x6000);
        assert_eq!(get_copy.flavor_type, *b"PICT");
        assert_eq!(get_copy.offset_ptr, 0x5004);
        assert!(!get_copy.is_query_only());

        // PutScrap
        let put_valid = evaluate_put_scrap_parameters(12, *b"TEXT", 0x7000);
        assert_eq!(put_valid.length, 12);
        assert_eq!(put_valid.flavor_type, *b"TEXT");
        assert_eq!(put_valid.source_ptr, 0x7000);
        assert!(put_valid.is_valid());
        assert_eq!(put_valid.data_length(), 12);

        let put_zero_len = evaluate_put_scrap_parameters(0, *b"TEXT", 0x7000);
        assert!(!put_zero_len.is_valid());
        assert_eq!(put_zero_len.data_length(), 0);

        let put_null_ptr = evaluate_put_scrap_parameters(12, *b"TEXT", 0);
        assert!(!put_null_ptr.is_valid());

        let put_neg_len = evaluate_put_scrap_parameters(-5, *b"TEXT", 0x7000);
        assert!(!put_neg_len.is_valid());
        assert_eq!(put_neg_len.data_length(), 0);

        // ZeroScrap, LoadScrap, UnloadScrap
        assert_eq!(evaluate_zero_scrap(), ZeroScrapAction);
        assert_eq!(evaluate_load_scrap(), LoadScrapAction);
        assert_eq!(evaluate_unload_scrap(), UnloadScrapAction);
    }
}
