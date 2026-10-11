//! Classic Macintosh International Utilities date and time formatting evaluation.
//!
//! Inside Macintosh Volume I, pp. I-487, I-504--I-506;
//! Inside Macintosh: Text (1993), Chapter 5, Appendix B (pp. B-23--B-26, B-29--B-32).

use crate::loader::ppc::memory::{ppc_memory_read_bytes, ppc_optional_pstring_output_can_write};
use crate::loader::ppc::ppc_write_pstring_bytes;
use crate::loader::ppc::PpcSectionMem;
use crate::trap::TrapDispatcher;
use ppc::PpcMemory;

// Inside Macintosh: Text (1993), pp. 5-86--5-87, B-23--B-26, B-29--B-32.
// DateString is the renamed IUDatePString; DateForm is an integer, not Boolean.
pub(crate) fn ppc_write_date_string(
    memory: &mut PpcSectionMem,
    seconds: u32,
    form: i16,
    result: u32,
    intl_handle: u32,
) {
    let resource = if intl_handle == 0 {
        TrapDispatcher::system_intl_default_body(if form == 0 { 0 } else { 1 })
    } else {
        memory
            .read_u32_be(intl_handle)
            .filter(|ptr| *ptr != 0)
            .and_then(|ptr| ppc_memory_read_bytes(memory, ptr, if form == 0 { 10 } else { 332 }))
    };
    let Some(bytes) = resource.and_then(|resource| format_date(seconds, form, &resource)) else {
        return;
    };
    if result != 0 && ppc_optional_pstring_output_can_write(memory, result, &bytes) {
        let _ = ppc_write_pstring_bytes(memory, result, &bytes);
    }
}

// TimeString is the renamed IUTimePString.
pub(crate) fn ppc_write_time_string(
    memory: &mut PpcSectionMem,
    seconds: u32,
    want_seconds: bool,
    result: u32,
    intl_handle: u32,
) {
    let resource = if intl_handle == 0 {
        TrapDispatcher::system_intl_default_body(0)
    } else {
        memory
            .read_u32_be(intl_handle)
            .filter(|ptr| *ptr != 0)
            .and_then(|ptr| {
                ppc_memory_read_bytes(memory, ptr, 32)
                    .or_else(|| ppc_memory_read_bytes(memory, ptr, 21))
            })
    };
    let Some(bytes) = resource.and_then(|resource| format_time(seconds, want_seconds, &resource))
    else {
        return;
    };
    if result != 0 && ppc_optional_pstring_output_can_write(memory, result, &bytes) {
        let _ = ppc_write_pstring_bytes(memory, result, &bytes);
    }
}

// Inside Macintosh: Text (1993), pp. 5-87--5-88, B-23--B-26.
// LongDateString formats a 64-bit LongDateTime record.
pub(crate) fn ppc_write_long_date_string(
    memory: &mut PpcSectionMem,
    date_time_ptr: u32,
    form: i16,
    result: u32,
    intl_handle: u32,
) {
    let seconds = if date_time_ptr != 0 {
        memory.read_u32_be(date_time_ptr + 4).unwrap_or(0)
    } else {
        0
    };
    ppc_write_date_string(memory, seconds, form, result, intl_handle);
}

// LongTimeString formats a 64-bit LongDateTime record.
pub(crate) fn ppc_write_long_time_string(
    memory: &mut PpcSectionMem,
    date_time_ptr: u32,
    want_seconds: bool,
    result: u32,
    intl_handle: u32,
) {
    let seconds = if date_time_ptr != 0 {
        memory.read_u32_be(date_time_ptr + 4).unwrap_or(0)
    } else {
        0
    };
    ppc_write_time_string(memory, seconds, want_seconds, result, intl_handle);
}

fn decimal(value: u16, width: usize) -> Vec<u8> {
    format!("{value:0width$}").into_bytes()
}

fn name(resource: &[u8], offset: usize, abbreviation: Option<usize>) -> Option<Vec<u8>> {
    let length = usize::from(*resource.get(offset)?);
    if length > 15 {
        return None;
    }
    let mut bytes = resource.get(offset + 1..offset + 1 + length)?.to_vec();
    if let Some(width) = abbreviation {
        if width > 15 {
            return None;
        }
        bytes.resize(width, 0);
    }
    Some(bytes)
}

fn separator(resource: &[u8], index: usize) -> Option<&[u8]> {
    let bytes = resource.get(308 + 4 * index..312 + 4 * index)?;
    Some(&bytes[..bytes.iter().position(|byte| *byte == 0).unwrap_or(4)])
}

fn suffix_string(bytes: &[u8]) -> &[u8] {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..len]
}

pub(crate) fn format_date(seconds: u32, form: i16, resource: &[u8]) -> Option<Vec<u8>> {
    let date = crate::time_manager::evaluate_seconds_to_date(seconds);
    if form == 0 {
        let flags = *resource.get(8)?;
        let month = decimal(date.month, if flags & 0x40 != 0 { 2 } else { 1 });
        let day = decimal(date.day, if flags & 0x20 != 0 { 2 } else { 1 });
        let year = decimal(
            if flags & 0x80 != 0 {
                date.year
            } else {
                date.year % 100
            },
            if flags & 0x80 != 0 { 4 } else { 2 },
        );
        let parts = [month, day, year];
        let order = match *resource.get(7)? {
            0 => [0, 1, 2],
            1 => [1, 0, 2],
            2 => [2, 0, 1],
            3 => [0, 2, 1],
            4 => [1, 2, 0],
            5 => [2, 1, 0],
            _ => return None,
        };
        let sep = *resource.get(9)?;
        let mut result = Vec::new();
        for (index, part) in order.into_iter().enumerate() {
            if index != 0 && sep != 0 {
                result.push(sep);
            }
            result.extend_from_slice(&parts[part]);
        }
        return Some(result);
    }
    if !matches!(form, 1 | 2) {
        return None;
    }
    // Extended calendars/name tables require their own interpreter. Do not
    // silently render an explicitly supplied extended resource as U.S. text.
    if resource.get(330..332)? == [0xA8, 0x9F] {
        return None;
    }
    let abbreviation = (form == 2).then_some(usize::from(*resource.get(307)?));
    let day_name = name(
        resource,
        usize::from(date.day_of_week - 1) * 16,
        abbreviation,
    )?;
    let month = name(
        resource,
        112 + usize::from(date.month - 1) * 16,
        abbreviation,
    )?;
    let day = decimal(date.day, if *resource.get(306)? == 255 { 2 } else { 1 });
    let parts = [day, day_name, month, decimal(date.year, 4)];
    let suppress = match *resource.get(304)? {
        255 => 2,
        value => value,
    };
    let order = match *resource.get(305)? {
        0 => [1, 0, 2, 3],
        255 => [1, 2, 0, 3],
        value => [value & 3, (value >> 2) & 3, (value >> 4) & 3, value >> 6],
    };
    let mut result = separator(resource, 0)?.to_vec();
    for (index, part) in order.into_iter().enumerate() {
        if suppress & (1 << part) == 0 {
            result.extend_from_slice(&parts[usize::from(part)]);
            result.extend_from_slice(separator(resource, index + 1)?);
        }
    }
    Some(result)
}

pub(crate) fn format_time(seconds: u32, want_seconds: bool, resource: &[u8]) -> Option<Vec<u8>> {
    let date = crate::time_manager::evaluate_seconds_to_date(seconds);
    let time_cycle = *resource.get(10)?;
    let time_fmt = *resource.get(11)?;
    let time_sep = *resource.get(20)?;

    let (hour, is_afternoon) = match time_cycle {
        0 => (date.hour, date.hour >= 12),
        1 => (date.hour % 12, date.hour >= 12),
        _ => {
            let h = date.hour % 12;
            (if h == 0 { 12 } else { h }, date.hour >= 12)
        }
    };

    let hour_str = decimal(hour, if time_fmt & 0x80 != 0 { 2 } else { 1 });
    let min_str = decimal(date.minute, if time_fmt & 0x40 != 0 { 2 } else { 1 });

    let mut result = Vec::new();
    result.extend_from_slice(&hour_str);
    if time_sep != 0 {
        result.push(time_sep);
    }
    result.extend_from_slice(&min_str);

    if want_seconds {
        let sec_str = decimal(date.second, if time_fmt & 0x20 != 0 { 2 } else { 1 });
        if time_sep != 0 {
            result.push(time_sep);
        }
        result.extend_from_slice(&sec_str);
    }

    let suffix = if time_cycle == 0 {
        let range = if is_afternoon { 25..29 } else { 21..25 };
        resource.get(range).map(suffix_string).unwrap_or_default()
    } else {
        let range = if is_afternoon { 16..20 } else { 12..16 };
        resource.get(range).map(suffix_string).unwrap_or_default()
    };
    result.extend_from_slice(suffix);

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(year: u16, month: u16, day: u16) -> u32 {
        crate::time_manager::evaluate_date_to_seconds(&crate::time_manager::DateTimeRecord {
            year,
            month,
            day,
            ..Default::default()
        })
    }

    fn time_stamp(hour: u16, minute: u16, second: u16) -> u32 {
        crate::time_manager::evaluate_date_to_seconds(&crate::time_manager::DateTimeRecord {
            year: 2024,
            month: 2,
            day: 9,
            hour,
            minute,
            second,
            ..Default::default()
        })
    }

    #[test]
    fn short_date_honors_all_orders_separator_padding_and_century() {
        let mut resource = TrapDispatcher::system_intl_default_body(0).unwrap();
        let stamp = seconds(2024, 2, 9);
        for (order, expected) in ["2/9/24", "9/2/24", "24/2/9", "2/24/9", "9/24/2", "24/9/2"]
            .into_iter()
            .enumerate()
        {
            resource[7] = order as u8;
            assert_eq!(
                format_date(stamp, 0, &resource).unwrap(),
                expected.as_bytes()
            );
        }
        resource[7] = 1;
        resource[8] = 0xE0;
        resource[9] = b'.';
        assert_eq!(format_date(stamp, 0, &resource).unwrap(), b"09.02.2024");
        resource[9] = 0;
        assert_eq!(format_date(stamp, 0, &resource).unwrap(), b"09022024");
        resource[7] = 6;
        assert!(format_date(stamp, 0, &resource).is_none());
    }

    #[test]
    fn roman_defaults_use_timestamp_leap_day_weekday_and_date_form() {
        let short = TrapDispatcher::system_intl_default_body(0).unwrap();
        let long = TrapDispatcher::system_intl_default_body(1).unwrap();
        assert_eq!(format_date(0, 0, &short).unwrap(), b"1/1/04");
        assert_eq!(
            format_date(seconds(2024, 1, 1), 0, &short).unwrap(),
            b"1/1/24"
        );
        assert_eq!(
            format_date(seconds(2024, 2, 29), 1, &long).unwrap(),
            b"Thursday, February 29, 2024"
        );
        assert_eq!(
            format_date(seconds(2024, 2, 29), 2, &long).unwrap(),
            b"Thu, Feb 29, 2024"
        );
        assert_eq!(
            format_date(seconds(2024, 3, 1), 1, &long).unwrap(),
            b"Friday, March 1, 2024"
        );
        assert!(format_date(0, 3, &long).is_none());
    }

    #[test]
    fn custom_long_resource_preserves_names_and_suppresses_matching_separator() {
        let mut resource = TrapDispatcher::system_intl_default_body(1).unwrap();
        resource[304] = 255; // omit weekday and its following separator
        resource[305] = 0; // day-month-year
        resource[306] = 255;
        resource[312..316].copy_from_slice(b"!\0\0\0");
        resource[320..324].copy_from_slice(b" / \0");
        let offset = 112 + 16;
        resource[offset] = 3;
        resource[offset + 1..offset + 4].copy_from_slice(&[0x8E, b'x', b'y']);
        assert_eq!(
            format_date(seconds(2024, 2, 9), 1, &resource).unwrap(),
            b"09 \x8Exy / 2024"
        );
        resource[307] = 5;
        assert_eq!(
            format_date(seconds(2024, 2, 9), 2, &resource).unwrap(),
            b"09 \x8Exy\0\0 / 2024"
        );
        resource[304] = 2 | 8; // omit weekday and year
        assert_eq!(
            format_date(seconds(2024, 2, 9), 1, &resource).unwrap(),
            b"09 \x8Exy / "
        );
        resource[330..332].copy_from_slice(&[0xA8, 0x9F]);
        assert!(format_date(0, 1, &resource).is_none());
    }

    #[test]
    fn time_formatting_honors_cycles_leading_zeros_and_am_pm_suffixes() {
        let mut resource = TrapDispatcher::system_intl_default_body(0).unwrap();

        // 12-hour defaults at midnight and noon
        assert_eq!(format_time(0, true, &resource).unwrap(), b"12:00:00 AM");
        assert_eq!(format_time(0, false, &resource).unwrap(), b"12:00 AM");
        let noon = time_stamp(12, 0, 0);
        assert_eq!(format_time(noon, true, &resource).unwrap(), b"12:00:00 PM");
        assert_eq!(format_time(noon, false, &resource).unwrap(), b"12:00 PM");

        // Afternoon time with single-digit hour, minutes, seconds
        let afternoon = time_stamp(13, 5, 9);
        assert_eq!(
            format_time(afternoon, true, &resource).unwrap(),
            b"1:05:09 PM"
        );
        assert_eq!(
            format_time(afternoon, false, &resource).unwrap(),
            b"1:05 PM"
        );

        // Leading zero for hours flag (0x80)
        resource[11] |= 0x80;
        assert_eq!(
            format_time(afternoon, true, &resource).unwrap(),
            b"01:05:09 PM"
        );

        // Omit leading zero for minutes and seconds
        resource[11] = 0;
        assert_eq!(
            format_time(afternoon, true, &resource).unwrap(),
            b"1:5:9 PM"
        );

        // 24-hour cycle (0) with and without custom suffix
        resource[10] = 0;
        resource[11] = 0x60;
        assert_eq!(
            format_time(afternoon, true, &resource).unwrap(),
            b"13:05:09"
        );
        assert_eq!(format_time(afternoon, false, &resource).unwrap(), b"13:05");
        // Evening suffix in 24-hour mode (bytes 25..29)
        resource[25..29].copy_from_slice(b" Uhr");
        assert_eq!(
            format_time(afternoon, true, &resource).unwrap(),
            b"13:05:09 Uhr"
        );
        let morning = time_stamp(9, 15, 30);
        resource[21..25].copy_from_slice(b" am\0");
        assert_eq!(
            format_time(morning, true, &resource).unwrap(),
            b"9:15:30 am"
        );

        // 12-hour cycle variant (timeCycleZero = 1): midnight and noon as 0
        resource[10] = 1;
        assert_eq!(format_time(0, true, &resource).unwrap(), b"0:00:00 AM");
        assert_eq!(format_time(noon, true, &resource).unwrap(), b"0:00:00 PM");

        // Custom time separator and omitted separator
        resource[10] = 0xFF;
        resource[20] = b'.';
        assert_eq!(format_time(0, true, &resource).unwrap(), b"12.00.00 AM");
        resource[20] = 0;
        assert_eq!(format_time(0, true, &resource).unwrap(), b"120000 AM");

        // Resource shorter than required minimum 21 bytes returns None
        assert!(format_time(0, true, &resource[..20]).is_none());
    }

    #[test]
    fn ppc_write_long_date_time_string_tests() {
        let mut memory = PpcSectionMem::default();
        let long_date_time_ptr = 0x1000;
        let result_date_ptr = 0x2000;
        let result_time_ptr = 0x3000;
        memory.add_region(long_date_time_ptr, vec![0; 16]);
        memory.add_region(result_date_ptr, vec![0; 256]);
        memory.add_region(result_time_ptr, vec![0; 256]);

        let timestamp =
            crate::time_manager::evaluate_date_to_seconds(&crate::time_manager::DateTimeRecord {
                year: 1971,
                month: 4,
                day: 18,
                hour: 12,
                minute: 0,
                second: 0,
                ..Default::default()
            });
        let _ = memory.write_u32_be(long_date_time_ptr, 0); // high 32 bits
        let _ = memory.write_u32_be(long_date_time_ptr + 4, timestamp); // low 32 bits

        ppc_write_long_date_string(&mut memory, long_date_time_ptr, 0, result_date_ptr, 0);
        let date_bytes =
            crate::loader::ppc::ppc_read_pstring_bytes(&mut memory, result_date_ptr).unwrap();
        assert_eq!(date_bytes, b"4/18/71");

        ppc_write_long_time_string(&mut memory, long_date_time_ptr, true, result_time_ptr, 0);
        let time_bytes =
            crate::loader::ppc::ppc_read_pstring_bytes(&mut memory, result_time_ptr).unwrap();
        assert_eq!(time_bytes, b"12:00:00 PM");
    }
}
