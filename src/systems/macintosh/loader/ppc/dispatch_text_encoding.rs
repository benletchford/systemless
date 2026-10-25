//! Text Encoding Converter conversions between Mac OS Roman and Unicode.
//!
//! TextEncodingConverter.h and TextCommon.h (Universal Interfaces 3.4,
//! CarbonLib): converter objects are opaque, and the Unicode side is
//! UTF-16 UniChar arrays. The runtime's text is Mac OS Roman, so that is the
//! only text encoding converters accept.

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

pub(crate) fn ppc_dispatch_text_encoding(
    operation: PpcTextEncodingOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    use PpcTextEncodingOperation as Op;
    let status = |value: i32| PpcImportAction::Return(value as u32);
    match operation {
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
