//! Classic Internet Config packed mapping preferences.
use super::*;

const BAD_MAPPING: i16 = -185; // MacErrors.h: badExtResource.
const FIXED_LENGTH: usize = 22; // InternetConfig.h: kICMapFixedLength.
const UNPACKED_LENGTH: u32 = 22 + 5 * 256;

fn record_length(bytes: &[u8]) -> Result<usize, i16> {
    let prefix = bytes.get(..2).ok_or(BAD_MAPPING)?;
    let length = usize::from(u16::from_be_bytes(prefix.try_into().unwrap()));
    if length < 2 || length > i16::MAX as usize || length > bytes.len() {
        return Err(BAD_MAPPING);
    }
    Ok(length)
}

fn unpack_record(bytes: &[u8]) -> Result<Vec<(u32, Vec<u8>)>, i16> {
    let length = record_length(bytes)?;
    let record = &bytes[..length];
    let fixed = record.get(..FIXED_LENGTH).ok_or(BAD_MAPPING)?;
    if u16::from_be_bytes(fixed[2..4].try_into().unwrap()) as usize != FIXED_LENGTH {
        return Err(BAD_MAPPING);
    }
    let mut writes = vec![(0, fixed.to_vec())];
    let mut cursor = FIXED_LENGTH;
    for field in 0..5 {
        let size = usize::from(*record.get(cursor).ok_or(BAD_MAPPING)?) + 1;
        let string = record.get(cursor..cursor + size).ok_or(BAD_MAPPING)?;
        writes.push((FIXED_LENGTH as u32 + field * 256, string.to_vec()));
        cursor += size;
    }
    // Remaining packed bytes are user data, deliberately not returned by
    // ICGetIndMapEntry or ICGetMapEntry (ICMapping.p: UnpackEntry).
    Ok(writes)
}

pub(super) fn dispatch(
    target: &PpcImportDispatcherTarget,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &ProcessNativeMemoryManager,
) -> i16 {
    let Some(record) = manager
        .native_handle_records()
        .iter()
        .find(|record| record.handle == cpu.gpr[4])
    else {
        return PPC_PARAM_ERR;
    };
    if record.ptr == 0 || memory.read_u32_be(record.handle) != Some(record.ptr) {
        return PPC_PARAM_ERR;
    }
    let Some(bytes) = ppc_memory_read_bytes(memory, record.ptr, record.size) else {
        return PPC_PARAM_ERR;
    };
    if *target == PpcImportDispatcherTarget::ICCountMapEntries {
        let output = cpu.gpr[5];
        if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
            return PPC_PARAM_ERR;
        }
        let mut offset = 0;
        let mut count = 0;
        while offset < bytes.len() {
            let length = match record_length(&bytes[offset..]) {
                Ok(length) => length,
                Err(error) => return error,
            };
            offset += length;
            count += 1;
        }
        let _ = memory.write_u32_be(output, count);
        return PPC_NO_ERR;
    }
    let (offset, position, output) = if *target == PpcImportDispatcherTarget::ICGetMapEntry {
        if (cpu.gpr[5] as i32) < 0 {
            return PPC_PARAM_ERR;
        }
        (cpu.gpr[5] as usize, None, cpu.gpr[6])
    } else {
        let index = cpu.gpr[5] as i32;
        let position = cpu.gpr[6];
        if index <= 0 || position == 0 || !ppc_memory_can_write_bytes(memory, position, 4) {
            return PPC_PARAM_ERR;
        }
        let mut offset = 0;
        for _ in 1..index {
            if offset == bytes.len() {
                return PPC_PARAM_ERR;
            }
            match record_length(&bytes[offset..]) {
                Ok(length) => offset += length,
                Err(error) => return error,
            }
        }
        (offset, Some(position), cpu.gpr[7])
    };
    if offset >= bytes.len()
        || output == 0
        || !ppc_memory_can_write_bytes(memory, output, UNPACKED_LENGTH)
    {
        return PPC_PARAM_ERR;
    }
    let writes = match unpack_record(&bytes[offset..]) {
        Ok(writes) => writes,
        Err(error) => return error,
    };
    for (relative, data) in writes {
        let _ = memory.write_bytes(output + relative, &data);
    }
    if let Some(position) = position {
        let _ = memory.write_u32_be(position, offset as u32);
    }
    PPC_NO_ERR
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truncated_mapping_strings_and_invalid_lengths_are_rejected() {
        let mut record = vec![0; 27];
        record[..2].copy_from_slice(&27u16.to_be_bytes());
        record[2..4].copy_from_slice(&22u16.to_be_bytes());
        let fields = unpack_record(&record).unwrap();
        assert_eq!(fields.len(), 6);
        assert_eq!(fields[5], (1046, vec![0]));
        for end in 0..record.len() {
            assert_eq!(unpack_record(&record[..end]), Err(BAD_MAPPING));
        }
        record[26] = 1;
        assert_eq!(unpack_record(&record), Err(BAD_MAPPING));
        record[..2].fill(0);
        assert_eq!(record_length(&record), Err(BAD_MAPPING));
        record[..2].fill(0xff);
        assert_eq!(record_length(&record), Err(BAD_MAPPING));
    }
}
