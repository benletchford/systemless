//! Text Encoding Converter conversions between Mac OS Roman and Unicode.
//!
//! TextEncodingConverter.h and TextCommon.h (Universal Interfaces 3.4,
//! CarbonLib): converter objects are opaque, and the Unicode side is
//! UTF-16 UniChar arrays. The runtime's text is Mac OS Roman, so that is the
//! only encoding supported by the older Unicode Converter entry points.
//! TEC objects also support stateless Unicode and Western single-byte codecs.

use super::*;

/// kTextEncodingMacRoman, with any variant and format bits ignored.
const TEXT_ENCODING_MAC_ROMAN: u32 = 0;
/// kTextUnsupportedEncodingErr.
const TEXT_UNSUPPORTED_ENCODING_ERR: i32 = -8738;
/// kTECOutputBufferFullStatus: the output buffer filled before the input
/// was consumed.
const TEC_OUTPUT_BUFFER_FULL_STATUS: i32 = -8785;
/// Opaque TextToUnicodeInfo and UnicodeToTextInfo values.
const TEXT_TO_UNICODE_INFO: u32 = 0x5445_4354; // 'TECT'
const UNICODE_TO_TEXT_INFO: u32 = 0x5445_4355; // 'TECU'

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcTextEncodingOperation {
    CreateEncoding,
    CreateConverter,
    DisposeConverter,
    ConvertText,
    FlushText,
    CreateTextToUnicodeInfo,
    CreateUnicodeToTextInfo,
    DisposeTextToUnicodeInfo,
    DisposeUnicodeToTextInfo,
    ConvertFromTextToUnicode,
    ConvertFromUnicodeToText,
    UpgradeScriptInfoToTextEncoding,
}

impl PpcTextEncodingOperation {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "CreateTextEncoding" => Self::CreateEncoding,
            "TECCreateConverter" => Self::CreateConverter,
            "TECDisposeConverter" => Self::DisposeConverter,
            "TECConvertText" => Self::ConvertText,
            "TECFlushText" => Self::FlushText,
            "CreateTextToUnicodeInfoByEncoding" => Self::CreateTextToUnicodeInfo,
            "CreateUnicodeToTextInfoByEncoding" => Self::CreateUnicodeToTextInfo,
            "DisposeTextToUnicodeInfo" => Self::DisposeTextToUnicodeInfo,
            "DisposeUnicodeToTextInfo" => Self::DisposeUnicodeToTextInfo,
            "ConvertFromTextToUnicode" => Self::ConvertFromTextToUnicode,
            "ConvertFromUnicodeToText" => Self::ConvertFromUnicodeToText,
            "UpgradeScriptInfoToTextEncoding" => Self::UpgradeScriptInfoToTextEncoding,
            _ => return None,
        })
    }
}

/// Arguments 9 to 12 of the twelve-argument conversion calls are in the
/// caller's parameter area.
fn stack_argument(cpu: &PpcCpu, memory: &mut PpcSectionMem, index: usize) -> u32 {
    ppc_parameter_area_slot_addr(cpu.gpr[1], index)
        .and_then(|slot| memory.read_u32_be(slot))
        .unwrap_or(0)
}

fn write_optional(memory: &mut PpcSectionMem, address: u32, value: u32) {
    if address != 0 {
        let _ = memory.write_u32_be(address, value);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TecCodec {
    MacRoman,
    Latin1,
    Windows1252,
    Ascii,
    Utf8,
    Utf16,
    Utf32,
}

impl TecCodec {
    fn from_encoding(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::MacRoman),
            0x201 => Some(Self::Latin1),
            0x500 => Some(Self::Windows1252),
            0x600 => Some(Self::Ascii),
            0x100 => Some(Self::Utf16),
            0x0800_0100 => Some(Self::Utf8),
            0x0c00_0100 => Some(Self::Utf32),
            _ => None,
        }
    }

    fn decode(self, bytes: &[u8]) -> Result<(char, usize), i32> {
        let partial = -8753;
        let malformed = -8739;
        match self {
            Self::Utf8 => {
                let width = match bytes[0] {
                    0..=0x7f => 1,
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Err(malformed),
                };
                let token = bytes.get(..width).ok_or(partial)?;
                let text = std::str::from_utf8(token).map_err(|_| malformed)?;
                Ok((text.chars().next().ok_or(malformed)?, width))
            }
            Self::Utf16 => {
                let word = |offset| {
                    bytes
                        .get(offset..offset + 2)
                        .map(|b| u16::from_be_bytes([b[0], b[1]]))
                        .ok_or(partial)
                };
                let first = word(0)?;
                if (0xd800..=0xdbff).contains(&first) {
                    let second = word(2)?;
                    if !(0xdc00..=0xdfff).contains(&second) {
                        return Err(malformed);
                    }
                    Ok((
                        char::from_u32(
                            0x10000 + ((u32::from(first) - 0xd800) << 10) + u32::from(second)
                                - 0xdc00,
                        )
                        .ok_or(malformed)?,
                        4,
                    ))
                } else {
                    Ok((char::from_u32(u32::from(first)).ok_or(malformed)?, 2))
                }
            }
            Self::Utf32 => {
                let b = bytes.get(..4).ok_or(partial)?;
                Ok((
                    char::from_u32(u32::from_be_bytes(b.try_into().unwrap())).ok_or(malformed)?,
                    4,
                ))
            }
            Self::MacRoman => Ok((decode_mac_roman(&bytes[..1]).chars().next().unwrap(), 1)),
            Self::Ascii if bytes[0] > 0x7f => Err(-8740),
            Self::Windows1252 if (0x80..0xa0).contains(&bytes[0]) => {
                char::from_u32(u32::from(WINDOWS_1252_HIGH[usize::from(bytes[0] - 0x80)]))
                    .filter(|&ch| ch != '\0')
                    .map(|ch| (ch, 1))
                    .ok_or(-8740)
            }
            _ => Ok((char::from(bytes[0]), 1)),
        }
    }

    fn encode(self, ch: char) -> Option<Vec<u8>> {
        match self {
            Self::Utf8 => Some(ch.encode_utf8(&mut [0; 4]).as_bytes().to_vec()),
            Self::Utf16 => Some(
                ch.encode_utf16(&mut [0; 2])
                    .iter()
                    .flat_map(|unit| unit.to_be_bytes())
                    .collect(),
            ),
            Self::Utf32 => Some((ch as u32).to_be_bytes().to_vec()),
            Self::MacRoman => crate::mac_roman::encode_mac_roman_char(ch).map(|byte| vec![byte]),
            Self::Ascii => ((ch as u32) < 128).then(|| vec![ch as u8]),
            Self::Latin1 => ((ch as u32) < 256).then(|| vec![ch as u8]),
            Self::Windows1252 => {
                let value = ch as u32;
                if value < 128 || (160..256).contains(&value) {
                    return Some(vec![value as u8]);
                }
                WINDOWS_1252_HIGH
                    .iter()
                    .position(|&unit| unit != 0 && u32::from(unit) == value)
                    .map(|index| vec![index as u8 + 0x80])
            }
        }
    }
}

const WINDOWS_1252_HIGH: [u16; 32] = [
    0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc,
    0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178,
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PpcTecState {
    next_id: u32,
    converters: std::collections::BTreeMap<u32, (TecCodec, TecCodec)>,
}

fn convert_tec(cpu: &PpcCpu, memory: &mut PpcSectionMem, state: &PpcTecState) -> i32 {
    let Some(&(source_codec, destination_codec)) = state.converters.get(&cpu.gpr[3]) else {
        return -8748;
    };
    let (source, source_length, read_out, output, capacity, written_out) = (
        cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8], cpu.gpr[9],
    );
    if read_out == 0
        || written_out == 0
        || !ppc_memory_can_write_bytes(memory, read_out, 4)
        || !ppc_memory_can_write_bytes(memory, written_out, 4)
        || !ppc_memory_can_write_bytes(memory, output, capacity)
    {
        return i32::from(PPC_PARAM_ERR);
    }
    let Some(input) = ppc_memory_read_bytes(memory, source, source_length) else {
        return i32::from(PPC_PARAM_ERR);
    };
    let mut consumed = 0;
    let mut result = Vec::new();
    let mut error = 0;
    while consumed < input.len() {
        let (ch, width) = match source_codec.decode(&input[consumed..]) {
            Ok(value) => value,
            Err(code) => {
                error = code;
                break;
            }
        };
        let Some(encoded) = destination_codec.encode(ch) else {
            error = -8754;
            break;
        };
        if encoded.len() > capacity as usize - result.len() {
            error = if consumed == 0 {
                -8750
            } else {
                TEC_OUTPUT_BUFFER_FULL_STATUS
            };
            break;
        }
        result.extend(encoded);
        consumed += width;
    }
    let _ = memory.write_bytes(output, &result);
    write_optional(memory, read_out, consumed as u32);
    write_optional(memory, written_out, result.len() as u32);
    error
}

pub(crate) fn ppc_dispatch_text_encoding(
    operation: PpcTextEncodingOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    state: &mut PpcTecState,
) -> PpcImportAction {
    use PpcTextEncodingOperation as Op;
    let status = |value: i32| PpcImportAction::Return(value as u32);
    match operation {
        Op::CreateEncoding => status(
            ((cpu.gpr[3] & 0xffff) | ((cpu.gpr[4] & 0x3ff) << 16) | ((cpu.gpr[5] & 0xf) << 26))
                as i32,
        ),
        Op::CreateConverter => {
            if cpu.gpr[3] == 0 || memory.write_u32_be(cpu.gpr[3], 0).is_none() {
                return status(i32::from(PPC_PARAM_ERR));
            }
            let Some(codecs) =
                TecCodec::from_encoding(cpu.gpr[4]).zip(TecCodec::from_encoding(cpu.gpr[5]))
            else {
                return status(-8749);
            };
            let Some(next) = state
                .next_id
                .checked_add(1)
                .filter(|value| *value < 0x10000)
            else {
                return status(i32::from(PPC_MEM_FULL_ERR));
            };
            let handle = 0x5443_0000 + next;
            state.next_id = next;
            state.converters.insert(handle, codecs);
            let _ = memory.write_u32_be(cpu.gpr[3], handle);
            status(0)
        }
        Op::DisposeConverter => status(if state.converters.remove(&cpu.gpr[3]).is_some() {
            0
        } else {
            -8748
        }),
        Op::ConvertText => status(convert_tec(cpu, memory, state)),
        Op::FlushText => {
            if !state.converters.contains_key(&cpu.gpr[3]) {
                return status(-8748);
            }
            // These codecs have no shift state or buffered suffix. Partial
            // source characters remain unconsumed for the caller to resubmit.
            if cpu.gpr[6] == 0 || memory.write_u32_be(cpu.gpr[6], 0).is_none() {
                return status(i32::from(PPC_PARAM_ERR));
            }
            status(0)
        }

        Op::CreateTextToUnicodeInfo | Op::CreateUnicodeToTextInfo => {
            // (TextEncoding encoding, …Info *outInfo). For Unicode-to-text the
            // argument names the target text encoding.
            if cpu.gpr[3] & 0xFFFF != TEXT_ENCODING_MAC_ROMAN {
                return status(TEXT_UNSUPPORTED_ENCODING_ERR);
            }
            let info = if operation == Op::CreateTextToUnicodeInfo {
                TEXT_TO_UNICODE_INFO
            } else {
                UNICODE_TO_TEXT_INFO
            };
            if cpu.gpr[4] == 0 || memory.write_u32_be(cpu.gpr[4], info).is_none() {
                return status(i32::from(PPC_PARAM_ERR));
            }
            status(0)
        }
        Op::DisposeTextToUnicodeInfo | Op::DisposeUnicodeToTextInfo => {
            write_optional(memory, cpu.gpr[3], 0);
            status(0)
        }
        Op::ConvertFromTextToUnicode => {
            // (info, ByteCount srcLen, src, OptionBits flags, ItemCount
            // offsetCount, ByteOffset *offsets, ItemCount *offsetOutCount,
            // ByteOffset *offsetOut, ByteCount bufLen, ByteCount *srcRead,
            // ByteCount *unicodeLen, UniChar *unicodeStr).
            if cpu.gpr[3] != TEXT_TO_UNICODE_INFO {
                return status(i32::from(PPC_PARAM_ERR));
            }
            let (length, source) = (cpu.gpr[4], cpu.gpr[5]);
            let buffer_length = stack_argument(cpu, memory, 8);
            let (read_out, length_out, output) = (
                stack_argument(cpu, memory, 9),
                stack_argument(cpu, memory, 10),
                stack_argument(cpu, memory, 11),
            );
            let Some(bytes) = ppc_memory_read_bytes(memory, source, length) else {
                return status(i32::from(PPC_PARAM_ERR));
            };
            let fits = (buffer_length / 2) as usize;
            let consumed = bytes.len().min(fits);
            for (index, byte) in bytes[..consumed].iter().enumerate() {
                let unit = decode_mac_roman(&[*byte])
                    .encode_utf16()
                    .next()
                    .unwrap_or(0xFFFD);
                if memory
                    .write_u16_be(output + index as u32 * 2, unit)
                    .is_none()
                {
                    return status(i32::from(PPC_PARAM_ERR));
                }
            }
            write_optional(memory, cpu.gpr[9], 0);
            write_optional(memory, read_out, consumed as u32);
            write_optional(memory, length_out, consumed as u32 * 2);
            status(if consumed < bytes.len() {
                TEC_OUTPUT_BUFFER_FULL_STATUS
            } else {
                0
            })
        }
        Op::ConvertFromUnicodeToText => {
            // (info, ByteCount unicodeLen, const UniChar *unicodeStr,
            // OptionBits flags, ItemCount offsetCount, ByteOffset *offsets,
            // ItemCount *offsetOutCount, ByteOffset *offsetOut, ByteCount
            // outputBufLen, ByteCount *inputRead, ByteCount *outputLen,
            // LogicalAddress outputStr). Characters with no Mac OS Roman
            // form become '?', the fallback character.
            if cpu.gpr[3] != UNICODE_TO_TEXT_INFO {
                return status(i32::from(PPC_PARAM_ERR));
            }
            let (length, source) = (cpu.gpr[4] & !1, cpu.gpr[5]);
            let buffer_length = stack_argument(cpu, memory, 8) as usize;
            let (read_out, length_out, output) = (
                stack_argument(cpu, memory, 9),
                stack_argument(cpu, memory, 10),
                stack_argument(cpu, memory, 11),
            );
            let Some(bytes) = ppc_memory_read_bytes(memory, source, length) else {
                return status(i32::from(PPC_PARAM_ERR));
            };
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            let mut text = Vec::with_capacity(units.len());
            let mut consumed_units = 0;
            for decoded in char::decode_utf16(units.iter().copied()) {
                let width = decoded.as_ref().map_or(1, |ch| ch.len_utf16());
                if text.len() == buffer_length {
                    break;
                }
                let byte = decoded
                    .ok()
                    .and_then(crate::mac_roman::encode_mac_roman_char)
                    .unwrap_or(b'?');
                text.push(byte);
                consumed_units += width;
            }
            if memory.write_bytes(output, &text).is_none() && !text.is_empty() {
                return status(i32::from(PPC_PARAM_ERR));
            }
            write_optional(memory, cpu.gpr[9], 0);
            write_optional(memory, read_out, consumed_units as u32 * 2);
            write_optional(memory, length_out, text.len() as u32);
            status(if consumed_units < units.len() {
                TEC_OUTPUT_BUFFER_FULL_STATUS
            } else {
                0
            })
        }
        Op::UpgradeScriptInfoToTextEncoding => {
            // (ScriptCode, LangCode, RegionCode, ConstStr255Param fontName,
            // TextEncoding *encoding). Every script resolves to Mac OS Roman,
            // the only script system the runtime provides.
            if cpu.gpr[7] == 0
                || memory
                    .write_u32_be(cpu.gpr[7], TEXT_ENCODING_MAC_ROMAN)
                    .is_none()
            {
                return status(i32::from(PPC_PARAM_ERR));
            }
            status(0)
        }
    }
}
