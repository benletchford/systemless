//! Apple Desktop Bus device-table and input-packet state.

use std::collections::VecDeque;

/// One entry in the ADB Manager's device table.
///
/// Inside Macintosh Volume V (1986), pp. V-367 to V-370.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AdbDeviceEntry {
    pub current_address: u8,
    pub handler_id: u8,
    pub original_address: u8,
    pub service_routine: u32,
    pub data_area: u32,
}

/// One Talk-register-0 delivery to a registered ADB service routine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingAdbPacket {
    pub packet: [u8; 3],
    pub service_routine: u32,
    pub data_area: u32,
    pub command: u8,
}

pub(crate) struct AdbManager {
    devices: [AdbDeviceEntry; 2],
    pending_packets: VecDeque<PendingAdbPacket>,
    standard_service_routine: u32,
    last_mouse_position: (i16, i16),
    last_mouse_button: bool,
}

impl AdbManager {
    pub(crate) fn new() -> Self {
        Self {
            devices: [
                AdbDeviceEntry {
                    current_address: 2,
                    handler_id: 2,
                    original_address: 2,
                    service_routine: 0,
                    data_area: 0,
                },
                AdbDeviceEntry {
                    current_address: 3,
                    handler_id: 1,
                    original_address: 3,
                    service_routine: 0,
                    data_area: 0,
                },
            ],
            pending_packets: VecDeque::new(),
            standard_service_routine: 0,
            last_mouse_position: (0, 0),
            last_mouse_button: false,
        }
    }

    pub(crate) fn device_count(&self) -> u8 {
        self.devices.len() as u8
    }

    pub(crate) fn device_by_index(&self, index: u8) -> Option<AdbDeviceEntry> {
        index
            .checked_sub(1)
            .and_then(|index| self.devices.get(index as usize))
            .copied()
    }

    pub(crate) fn device_by_address(&self, address: u8) -> Option<AdbDeviceEntry> {
        self.devices
            .iter()
            .find(|entry| entry.current_address == address)
            .copied()
    }

    pub(crate) fn set_device_handler(
        &mut self,
        address: u8,
        service_routine: u32,
        data_area: u32,
        mouse_button: bool,
    ) -> bool {
        let Some(entry) = self
            .devices
            .iter_mut()
            .find(|entry| entry.current_address == address)
        else {
            return false;
        };
        entry.service_routine = service_routine;
        entry.data_area = data_area;
        if entry.original_address == 3 {
            // Frontends provide absolute host coordinates, while a custom
            // ADB driver consumes relative deltas and owns its cursor state.
            // Reset the bridge so the first host position is delivered as a
            // complete delta stream rather than relative to the HLE standard
            // driver's hidden host-side baseline.
            self.last_mouse_position = (0, 0);
            self.last_mouse_button = mouse_button;
            self.pending_packets.clear();
        }
        true
    }

    pub(crate) fn install_standard_service_routine(&mut self, service_routine: u32) {
        self.standard_service_routine = service_routine;
        for entry in &mut self.devices {
            entry.service_routine = service_routine;
        }
    }

    pub(crate) fn flush(&mut self, address: u8) {
        self.pending_packets
            .retain(|packet| packet.command >> 4 != address);
    }

    pub(crate) fn note_mouse_state(&mut self, position: (i16, i16), button: bool) {
        let Some(mouse) = self.device_by_address(3) else {
            return;
        };
        let mut remaining_v = i32::from(position.0) - i32::from(self.last_mouse_position.0);
        let mut remaining_h = i32::from(position.1) - i32::from(self.last_mouse_position.1);
        let button_changed = button != self.last_mouse_button;
        self.last_mouse_position = position;
        self.last_mouse_button = button;

        if mouse.service_routine == 0 || mouse.service_routine == self.standard_service_routine {
            return;
        }

        let mut first = true;
        while first || remaining_v != 0 || remaining_h != 0 {
            first = false;
            if remaining_v == 0 && remaining_h == 0 && !button_changed {
                break;
            }
            let delta_v = remaining_v.clamp(-64, 63) as i8;
            let delta_h = remaining_h.clamp(-64, 63) as i8;
            remaining_v -= i32::from(delta_v);
            remaining_h -= i32::from(delta_h);

            // A standard 100/200-dpi mouse returns two seven-bit signed
            // deltas. Bit 7 is clear while the corresponding button is down;
            // the one-button mouse always reports its second button as up.
            // Inside Macintosh Volume V (1986), pp. V-364 to V-366.
            let vertical = (delta_v as u8 & 0x7F) | if button { 0x00 } else { 0x80 };
            let horizontal = (delta_h as u8 & 0x7F) | 0x80;
            self.pending_packets.push_back(PendingAdbPacket {
                packet: [2, vertical, horizontal],
                service_routine: mouse.service_routine,
                data_area: mouse.data_area,
                command: (mouse.current_address << 4) | 0x0C,
            });
        }
    }

    pub(crate) fn pop_pending_packet(&mut self) -> Option<PendingAdbPacket> {
        self.pending_packets.pop_front()
    }

    #[cfg(test)]
    pub(crate) fn pending_packet_count(&self) -> usize {
        self.pending_packets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_motion_is_split_into_signed_seven_bit_talk_zero_packets() {
        let mut adb = AdbManager::new();
        assert!(adb.set_device_handler(3, 0x0012_3456, 0x0065_4321, false));

        adb.note_mouse_state((-80, 80), false);

        assert_eq!(adb.pending_packet_count(), 2);
        assert_eq!(
            adb.pop_pending_packet(),
            Some(PendingAdbPacket {
                packet: [2, 0xC0, 0xBF],
                service_routine: 0x0012_3456,
                data_area: 0x0065_4321,
                command: 0x3C,
            })
        );
        assert_eq!(
            adb.pop_pending_packet(),
            Some(PendingAdbPacket {
                packet: [2, 0xF0, 0x91],
                service_routine: 0x0012_3456,
                data_area: 0x0065_4321,
                command: 0x3C,
            })
        );
    }

    #[test]
    fn mouse_button_transitions_queue_zero_delta_packets() {
        let mut adb = AdbManager::new();
        assert!(adb.set_device_handler(3, 0x0012_3456, 0, false));

        adb.note_mouse_state((0, 0), true);
        adb.note_mouse_state((0, 0), false);

        assert_eq!(adb.pop_pending_packet().unwrap().packet, [2, 0x00, 0x80]);
        assert_eq!(adb.pop_pending_packet().unwrap().packet, [2, 0x80, 0x80]);
    }

    #[test]
    fn standard_mouse_service_does_not_duplicate_event_manager_input() {
        let mut adb = AdbManager::new();
        adb.install_standard_service_routine(0x0012_3456);

        adb.note_mouse_state((100, 200), true);

        assert_eq!(adb.pending_packet_count(), 0);
    }

    #[test]
    fn flush_discards_only_packets_for_the_selected_device() {
        let mut adb = AdbManager::new();
        assert!(adb.set_device_handler(3, 0x0012_3456, 0, false));
        adb.note_mouse_state((10, 20), false);
        adb.pending_packets.push_back(PendingAdbPacket {
            packet: [1, 0, 0],
            service_routine: 0x0065_4321,
            data_area: 0,
            command: 0x2C,
        });

        adb.flush(3);

        assert_eq!(adb.pending_packet_count(), 1);
        assert_eq!(adb.pop_pending_packet().unwrap().command, 0x2C);
    }
}
