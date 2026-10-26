//! Open Transport for a machine with no network configured.
//!
//! Open Transport itself initialises, but no endpoint, mapper, or services
//! provider can be opened, so every provider operation reports a bad
//! reference. The address, list, atomic, and timing utilities behave as in
//! OpenTransport.h and OpenTransportProviders.h (Universal Interfaces 3.4).

use super::*;

pub(crate) const PPC_OT_BAD_ADDRESS_ERR: i32 = -3150;
pub(crate) const PPC_OT_BAD_REFERENCE_ERR: i32 = -3153;
pub(crate) const PPC_OT_NOT_FOUND_ERR: i32 = -3201;
/// OTCreateConfiguration's opaque result. Callers only pass it back to Open
/// Transport, which never dereferences it here.
pub(crate) const PPC_OT_CONFIGURATION: u32 = PPC_IMPORT_DATA_BASE + 0x450;
/// The client context written by InitOpenTransportInContext.
pub(crate) const PPC_OT_CLIENT_CONTEXT: u32 = PPC_IMPORT_DATA_BASE + 0x454;
const AF_INET: u16 = 2;
const AF_DNS: u16 = 42;
const MAX_LIST_LINKS: usize = 0x10000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcOpenTransportFunction {
    /// Initialisation that succeeds with nothing to do.
    Succeed,
    /// Procedures with no result.
    Void,
    /// Functions returning their first argument unchanged.
    ReturnFirst,
    /// Boolean or pointer functions answering false or nil.
    ReturnZero,
    /// OTIsSynchronous and OTIsBlocking.
    ReturnTrue,
    InitInContext,
    CreateConfiguration,
    /// Synchronous opens: nil provider, error through the given argument.
    OpenProvider {
        error_arg: usize,
    },
    /// Asynchronous opens and interface queries: no provider is found.
    NotFound,
    /// Calls on a provider, mapper, or services reference.
    BadReference,
    InetHostToString,
    InetStringToHost,
    InitInetAddress,
    InitDnsAddress,
    InitDdpAddress,
    InitNbpEntity,
    SetNbpEntityFromAddress,
    SetNbpPart {
        index: u32,
    },
    ExtractNbpPart {
        index: u32,
    },
    NbpAddressLength,
    AddressFromNbpEntity,
    AtomicAdd {
        width: u8,
    },
    AtomicSetBit,
    AtomicClearBit,
    AtomicTestBit,
    CompareAndSwap32,
    LifoEnqueue,
    LifoDequeue,
    LifoStealList,
    ReverseList,
    AddLast,
    RemoveLast,
    GetFirst,
    GetLast,
    IsInList,
    RemoveLink,
    SubtractTimeStamps,
    TimeStampIn {
        milliseconds: bool,
    },
}

impl PpcOpenTransportFunction {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "InitOpenTransport" | "InitOpenTransportPriv" | "InitOpenTransportUtilities" => {
                Self::Succeed
            }
            "InitOpenTransportInContext" => Self::InitInContext,
            "CloseOpenTransport"
            | "CloseOpenTransportPriv"
            | "CloseOpenTransportInContext"
            | "OTExitPatchPriv"
            | "OTDestroyConfiguration"
            | "OTRemoveNotifier"
            | "OTLeaveNotifier"
            | "OTIdle"
            | "OTEnterInterrupt"
            | "OTLeaveInterrupt"
            | "OTDestroyTimerTask"
            | "OTDestroySystemTask"
            | "DisposeOTNotifyUPP" => Self::Void,
            "OTCloneConfiguration" => Self::ReturnFirst,
            "OTEnterNotifier"
            | "OTIsAckingSends"
            | "OTFindPortByRef"
            | "OTCreateTimerTask"
            | "OTCreateTimerTaskInContext"
            | "OTCreateSystemTaskPriv"
            | "OTScheduleTimerTask"
            | "OTScheduleSystemTask"
            | "OTCancelTimerTask"
            | "OTCancelSystemTask" => Self::ReturnZero,
            "OTIsSynchronous" | "OTIsBlocking" => Self::ReturnTrue,
            "OTCreateConfiguration" => Self::CreateConfiguration,
            // OTOpenEndpoint(config, oflag, TEndpointInfo *info, OSStatus *err).
            "OTOpenEndpointPriv" | "OTOpenEndpointInContext" => Self::OpenProvider { error_arg: 3 },
            // OTOpenMapper(config, oflag, OSStatus *err) and the services opens.
            "OTOpenMapperPriv"
            | "OTOpenMapperInContext"
            | "OTOpenInternetServicesPriv"
            | "OTOpenInternetServicesInContext"
            | "OTOpenAppleTalkServicesPriv"
            | "OTOpenAppleTalkServicesInContext" => Self::OpenProvider { error_arg: 2 },
            "OTAsyncOpenEndpointPriv"
            | "OTAsyncOpenEndpointInContext"
            | "OTAsyncOpenMapperPriv"
            | "OTAsyncOpenMapperInContext"
            | "OTAsyncOpenInternetServicesPriv"
            | "OTAsyncOpenInternetServicesInContext"
            | "OTAsyncOpenAppleTalkServicesPriv"
            | "OTAsyncOpenAppleTalkServicesInContext"
            | "OTInetGetInterfaceInfo"
            | "OTInetGetSecondaryAddresses" => Self::NotFound,
            "OTCloseProvider"
            | "OTCloseProviderPriv"
            | "OTSetSynchronous"
            | "OTSetAsynchronous"
            | "OTSetBlocking"
            | "OTSetNonBlocking"
            | "OTInstallNotifier"
            | "OTUseSyncIdleEvents"
            | "OTIoctl"
            | "OTAckSends"
            | "OTDontAckSends"
            | "OTCancelSynchronousCalls"
            | "OTGetEndpointInfo"
            | "OTGetEndpointState"
            | "OTLook"
            | "OTCountDataBytes"
            | "OTGetProtAddress"
            | "OTResolveAddress"
            | "OTBind"
            | "OTUnbind"
            | "OTConnect"
            | "OTRcvConnect"
            | "OTListen"
            | "OTAccept"
            | "OTSndDisconnect"
            | "OTSndOrderlyDisconnect"
            | "OTRcvDisconnect"
            | "OTRcvOrderlyDisconnect"
            | "OTRcv"
            | "OTSnd"
            | "OTRcvUData"
            | "OTSndUData"
            | "OTRcvUDErr"
            | "OTOptionManagement"
            | "OTRegisterName"
            | "OTDeleteName"
            | "OTDeleteNameByID"
            | "OTLookupName"
            | "OTInetStringToAddress"
            | "OTInetAddressToName"
            | "OTInetSysInfo"
            | "OTInetMailExchange"
            | "OTInetQuery"
            | "OTATalkGetMyZone"
            | "OTATalkGetLocalZones"
            | "OTATalkGetZoneList"
            | "OTATalkGetInfo" => Self::BadReference,
            "OTInetHostToString" => Self::InetHostToString,
            "OTInetStringToHost" => Self::InetStringToHost,
            "OTInitInetAddress" => Self::InitInetAddress,
            "OTInitDNSAddress" => Self::InitDnsAddress,
            "OTInitDDPAddress" => Self::InitDdpAddress,
            "OTInitNBPEntity" => Self::InitNbpEntity,
            "OTSetNBPEntityFromAddress" => Self::SetNbpEntityFromAddress,
            "OTSetNBPName" => Self::SetNbpPart { index: 0 },
            "OTSetNBPType" => Self::SetNbpPart { index: 1 },
            "OTSetNBPZone" => Self::SetNbpPart { index: 2 },
            "OTExtractNBPName" => Self::ExtractNbpPart { index: 0 },
            "OTExtractNBPType" => Self::ExtractNbpPart { index: 1 },
            "OTExtractNBPZone" => Self::ExtractNbpPart { index: 2 },
            "OTGetNBPEntityLengthAsAddress" => Self::NbpAddressLength,
            "OTSetAddressFromNBPEntity" => Self::AddressFromNbpEntity,
            "OTAtomicAdd32" => Self::AtomicAdd { width: 4 },
            "OTAtomicAdd16" => Self::AtomicAdd { width: 2 },
            "OTAtomicAdd8" => Self::AtomicAdd { width: 1 },
            "OTAtomicSetBit" => Self::AtomicSetBit,
            "OTAtomicClearBit" => Self::AtomicClearBit,
            "OTAtomicTestBit" => Self::AtomicTestBit,
            "OTCompareAndSwap32" | "OTCompareAndSwapPtr" => Self::CompareAndSwap32,
            "OTLIFOEnqueue" | "OTAddFirst" => Self::LifoEnqueue,
            "OTLIFODequeue" | "OTRemoveFirst" => Self::LifoDequeue,
            "OTLIFOStealList" => Self::LifoStealList,
            "OTReverseList" => Self::ReverseList,
            "OTAddLast" => Self::AddLast,
            "OTRemoveLast" => Self::RemoveLast,
            "OTGetFirst" => Self::GetFirst,
            "OTGetLast" => Self::GetLast,
            "OTIsInList" => Self::IsInList,
            "OTRemoveLink" => Self::RemoveLink,
            "OTSubtractTimeStamps" => Self::SubtractTimeStamps,
            "OTTimeStampInMilliseconds" => Self::TimeStampIn { milliseconds: true },
            "OTTimeStampInMicroseconds" => Self::TimeStampIn {
                milliseconds: false,
            },
            _ => return None,
        })
    }
}

// NBPEntity is opaque to clients. Keep each component as a length byte
// followed by up to 32 bytes within its 99-byte storage.
fn nbp_part(memory: &mut PpcSectionMem, entity: u32, index: u32) -> Vec<u8> {
    let address = entity.wrapping_add(index * 33);
    let len = memory.read_u8(address).unwrap_or(0).min(32);
    (0..u32::from(len))
        .map(|offset| {
            memory
                .read_u8(address.wrapping_add(1 + offset))
                .unwrap_or(0)
        })
        .collect()
}

fn nbp_store_part(memory: &mut PpcSectionMem, entity: u32, index: u32, bytes: &[u8]) -> bool {
    let address = entity.wrapping_add(index * 33);
    let len = bytes.len().min(32);
    let mut part = [0u8; 33];
    part[0] = len as u8;
    part[1..1 + len].copy_from_slice(&bytes[..len]);
    memory.write_bytes(address, &part).is_some() && bytes.len() <= 32
}

fn nbp_escape(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::new();
    for &byte in bytes {
        if matches!(byte, b'\\' | b':' | b'@') {
            escaped.push(b'\\');
        }
        escaped.push(byte);
    }
    escaped
}

fn nbp_address(memory: &mut PpcSectionMem, entity: u32) -> Vec<u8> {
    let mut address = nbp_escape(&nbp_part(memory, entity, 0));
    address.push(b':');
    address.extend(nbp_escape(&nbp_part(memory, entity, 1)));
    address.push(b'@');
    address.extend(nbp_escape(&nbp_part(memory, entity, 2)));
    address
}

fn read_u32(memory: &mut PpcSectionMem, address: u32) -> u32 {
    memory.read_u32_be(address).unwrap_or(0)
}

/// Walk an OTList or OTLIFO from its head, bounded against cycles.
fn list_links(memory: &mut PpcSectionMem, list: u32) -> Vec<u32> {
    let mut links = Vec::new();
    let mut link = read_u32(memory, list);
    while link != 0 && links.len() < MAX_LIST_LINKS && !links.contains(&link) {
        links.push(link);
        link = read_u32(memory, link);
    }
    links
}

fn read_time_stamp(memory: &mut PpcSectionMem, address: u32) -> u64 {
    (u64::from(read_u32(memory, address)) << 32) | u64::from(read_u32(memory, address + 4))
}

/// OTElapsedMilliseconds and OTElapsedMicroseconds: time stamps share the
/// Microseconds clock.
pub(crate) fn ppc_ot_elapsed(
    memory: &mut PpcSectionMem,
    start: u32,
    now: u64,
    milliseconds: bool,
) -> u32 {
    let elapsed = now.saturating_sub(read_time_stamp(memory, start));
    (if milliseconds {
        elapsed / 1000
    } else {
        elapsed
    }) as u32
}

pub(crate) fn ppc_dispatch_open_transport(
    function: PpcOpenTransportFunction,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    use PpcOpenTransportFunction as F;
    let status = |value: i32| PpcImportAction::Return(value as u32);
    let args = cpu.gpr;
    match function {
        F::Succeed => status(0),
        F::Void => PpcImportAction::ReturnPreserve,
        F::ReturnFirst => PpcImportAction::Return(args[3]),
        F::ReturnZero => PpcImportAction::Return(0),
        F::ReturnTrue => PpcImportAction::Return(1),
        F::InitInContext => {
            // InitOpenTransportInContext(flags, OTClientContextPtr *).
            if args[4] != 0 {
                let _ = memory.write_u32_be(args[4], PPC_OT_CLIENT_CONTEXT);
            }
            status(0)
        }
        F::CreateConfiguration => PpcImportAction::Return(PPC_OT_CONFIGURATION),
        F::OpenProvider { error_arg } => {
            let error = args[3 + error_arg];
            if error != 0 {
                let _ = memory.write_u32_be(error, PPC_OT_NOT_FOUND_ERR as u32);
            }
            PpcImportAction::Return(0)
        }
        F::NotFound => status(PPC_OT_NOT_FOUND_ERR),
        F::BadReference => status(PPC_OT_BAD_REFERENCE_ERR),
        F::InetHostToString => {
            // OTInetHostToString(InetHost, char *): dotted decimal.
            let text = std::net::Ipv4Addr::from(args[3]).to_string();
            let _ = memory.write_bytes(args[4], text.as_bytes());
            let _ = memory.write_u8(args[4] + text.len() as u32, 0);
            PpcImportAction::ReturnPreserve
        }
        F::InetStringToHost => {
            // OTInetStringToHost(const char *, InetHost *).
            let text = super::dispatch_stdc::ppc_std_c_string(memory, args[3], 256);
            let host = std::str::from_utf8(&text)
                .ok()
                .and_then(|text| text.trim().parse::<std::net::Ipv4Addr>().ok());
            match host {
                Some(host) if args[4] != 0 => {
                    let _ = memory.write_u32_be(args[4], u32::from(host));
                    status(0)
                }
                _ => status(PPC_OT_BAD_ADDRESS_ERR),
            }
        }
        F::InitInetAddress => {
            // InetAddress: fAddressType, fPort, fHost, then 8 unused bytes.
            let address = args[3];
            let _ = memory.write_u16_be(address, AF_INET);
            let _ = memory.write_u16_be(address + 2, args[4] as u16);
            let _ = memory.write_u32_be(address + 4, args[5]);
            let _ = memory.write_bytes(address + 8, &[0; 8]);
            PpcImportAction::ReturnPreserve
        }
        F::InitDdpAddress => {
            // Universal Interfaces DDPAddress: 16-bit type/network then
            // 8-bit node, socket, protocol type, and padding.
            let mut address = [0u8; 8];
            address[..2].copy_from_slice(&0x0100u16.to_be_bytes());
            address[2..4].copy_from_slice(&(args[4] as u16).to_be_bytes());
            address[4] = args[5] as u8;
            address[5] = args[6] as u8;
            address[6] = args[7] as u8;
            let _ = memory.write_bytes(args[3], &address);
            PpcImportAction::ReturnPreserve
        }
        F::InitNbpEntity => {
            let _ = memory.write_bytes(args[3], &[0; 99]);
            PpcImportAction::ReturnPreserve
        }
        F::SetNbpPart { index } => {
            let bytes = super::dispatch_stdc::ppc_std_c_string(memory, args[4], 33);
            PpcImportAction::Return(u32::from(nbp_store_part(memory, args[3], index, &bytes)))
        }
        F::ExtractNbpPart { index } => {
            let mut bytes = nbp_escape(&nbp_part(memory, args[3], index));
            bytes.push(0);
            let _ = memory.write_bytes(args[4], &bytes);
            PpcImportAction::ReturnPreserve
        }
        F::SetNbpEntityFromAddress => {
            // The pointer is the string portion of the NBP/DDPNBP address,
            // not its address-family header (OpenTransportProviders.h).
            let mut parts: [Vec<u8>; 3] = Default::default();
            let mut index = 0;
            let mut escaped = false;
            let mut complete = args[5] <= 0x10000;
            for offset in 0..args[5].min(0x10000) {
                let Some(byte) = args[4].checked_add(offset).and_then(|p| memory.read_u8(p)) else {
                    complete = false;
                    break;
                };
                if !escaped && byte == b'\\' {
                    escaped = true;
                    continue;
                }
                if !escaped && ((index == 0 && byte == b':') || (index < 2 && byte == b'@')) {
                    index = if byte == b':' { 1 } else { 2 };
                } else if parts[index].len() < 32 {
                    parts[index].push(byte);
                } else {
                    complete = false;
                }
                escaped = false;
            }
            if escaped {
                if parts[index].len() < 32 {
                    parts[index].push(b'\\');
                } else {
                    complete = false;
                }
            }
            for (index, part) in parts.iter().enumerate() {
                complete &= nbp_store_part(memory, args[3], index as u32, part);
            }
            PpcImportAction::Return(u32::from(complete))
        }
        F::NbpAddressLength => PpcImportAction::Return(nbp_address(memory, args[3]).len() as u32),
        F::AddressFromNbpEntity => {
            let address = nbp_address(memory, args[4]);
            let _ = memory.write_bytes(args[3], &address);
            PpcImportAction::Return(address.len() as u32)
        }
        F::InitDnsAddress => {
            // DNSAddress: fAddressType then the NUL-terminated name; the
            // result is the address length.
            let name = super::dispatch_stdc::ppc_std_c_string(memory, args[4], 255);
            let _ = memory.write_u16_be(args[3], AF_DNS);
            let _ = memory.write_bytes(args[3] + 2, &name);
            let _ = memory.write_u8(args[3] + 2 + name.len() as u32, 0);
            PpcImportAction::Return(name.len() as u32 + 2)
        }
        F::AtomicAdd { width } => {
            // OTAtomicAdd32(SInt32 toAdd, SInt32 *where): the new value.
            let (delta, address) = (args[3], args[4]);
            let value = match width {
                4 => {
                    let value = read_u32(memory, address).wrapping_add(delta);
                    let _ = memory.write_u32_be(address, value);
                    value as i32
                }
                2 => {
                    let value = memory
                        .read_u16_be(address)
                        .unwrap_or(0)
                        .wrapping_add(delta as u16);
                    let _ = memory.write_u16_be(address, value);
                    i32::from(value as i16)
                }
                _ => {
                    let value = memory
                        .read_u8(address)
                        .unwrap_or(0)
                        .wrapping_add(delta as u8);
                    let _ = memory.write_u8(address, value);
                    i32::from(value as i8)
                }
            };
            status(value)
        }
        F::AtomicSetBit | F::AtomicClearBit | F::AtomicTestBit => {
            // (UInt8 *bytePtr, OTByteCount bitNumber): bit 0 is the least
            // significant bit; the result is the bit's previous state.
            let (address, mask) = (args[3], 1u8 << (args[4] & 7));
            let byte = memory.read_u8(address).unwrap_or(0);
            match function {
                F::AtomicSetBit => {
                    let _ = memory.write_u8(address, byte | mask);
                }
                F::AtomicClearBit => {
                    let _ = memory.write_u8(address, byte & !mask);
                }
                _ => {}
            }
            PpcImportAction::Return(u32::from(byte & mask != 0))
        }
        F::CompareAndSwap32 => {
            // OTCompareAndSwap32(UInt32 old, UInt32 new, UInt32 *where).
            let swapped = read_u32(memory, args[5]) == args[3];
            if swapped {
                let _ = memory.write_u32_be(args[5], args[4]);
            }
            PpcImportAction::Return(u32::from(swapped))
        }
        F::LifoEnqueue => {
            let (list, link) = (args[3], args[4]);
            let head = read_u32(memory, list);
            let _ = memory.write_u32_be(link, head);
            let _ = memory.write_u32_be(list, link);
            PpcImportAction::ReturnPreserve
        }
        F::LifoDequeue => {
            let list = args[3];
            let head = read_u32(memory, list);
            if head != 0 {
                let next = read_u32(memory, head);
                let _ = memory.write_u32_be(list, next);
                let _ = memory.write_u32_be(head, 0);
            }
            PpcImportAction::Return(head)
        }
        F::LifoStealList => {
            let head = read_u32(memory, args[3]);
            let _ = memory.write_u32_be(args[3], 0);
            PpcImportAction::Return(head)
        }
        F::ReverseList => {
            // OTReverseList(OTLink *): reverse a chain and return its new head.
            let (mut previous, mut link, mut steps) = (0, args[3], 0);
            while link != 0 && steps < MAX_LIST_LINKS {
                let next = read_u32(memory, link);
                let _ = memory.write_u32_be(link, previous);
                previous = link;
                link = next;
                steps += 1;
            }
            PpcImportAction::Return(previous)
        }
        F::AddLast => {
            let (list, link) = (args[3], args[4]);
            let _ = memory.write_u32_be(link, 0);
            let tail = list_links(memory, list).last().copied().unwrap_or(list);
            let _ = memory.write_u32_be(tail, link);
            PpcImportAction::ReturnPreserve
        }
        F::RemoveLast => {
            let list = args[3];
            let links = list_links(memory, list);
            let Some(&last) = links.last() else {
                return PpcImportAction::Return(0);
            };
            let previous = links
                .len()
                .checked_sub(2)
                .map_or(list, |index| links[index]);
            let _ = memory.write_u32_be(previous, 0);
            PpcImportAction::Return(last)
        }
        F::GetFirst => PpcImportAction::Return(read_u32(memory, args[3])),
        F::GetLast => {
            PpcImportAction::Return(list_links(memory, args[3]).last().copied().unwrap_or(0))
        }
        F::IsInList => {
            PpcImportAction::Return(u32::from(list_links(memory, args[3]).contains(&args[4])))
        }
        F::RemoveLink => {
            let (list, link) = (args[3], args[4]);
            let links = list_links(memory, list);
            let Some(index) = links.iter().position(|&candidate| candidate == link) else {
                return PpcImportAction::Return(0);
            };
            let previous = index.checked_sub(1).map_or(list, |index| links[index]);
            let next = read_u32(memory, link);
            let _ = memory.write_u32_be(previous, next);
            let _ = memory.write_u32_be(link, 0);
            PpcImportAction::Return(1)
        }
        F::SubtractTimeStamps => {
            // OTSubtractTimeStamps(result, startTime, endTime) -> result.
            let end = read_time_stamp(memory, args[5]);
            let difference = end.wrapping_sub(read_time_stamp(memory, args[4]));
            let _ = memory.write_u32_be(args[3], (difference >> 32) as u32);
            let _ = memory.write_u32_be(args[3] + 4, difference as u32);
            PpcImportAction::Return(args[3])
        }
        F::TimeStampIn { milliseconds } => {
            let value = read_time_stamp(memory, args[3]);
            PpcImportAction::Return((if milliseconds { value / 1000 } else { value }) as u32)
        }
    }
}
