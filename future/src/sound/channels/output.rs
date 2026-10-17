//! Virtual output-device volume, IM Sound pp. 2-139–142. Low word left, high right.
use super::*;
impl Manager {
    pub fn get_output_volume(&self, m: &mut Memory, destination: GuestPtr) -> Result<i16> {
        if destination.0 == 0 {
            return Ok(-50);
        }
        let master = u32::from(super::super::volume(m)?);
        let left = u32::from(self.output_gain[0]) * master / 7;
        let right = u32::from(self.output_gain[1]) * master / 7;
        m.write(destination, &(left | (right << 16)).to_be_bytes())?;
        Ok(0)
    }
    pub fn set_output_volume(&mut self, m: &mut Memory, level: u32) -> Result<i16> {
        let gain = [level as u16, (level >> 16) as u16];
        if gain.iter().any(|v| *v > 256) {
            return Ok(-50);
        }
        // Normalize the coarse legacy master so a previous mute cannot silently
        // defeat a newly requested device level. Preflight before publishing gain.
        m.validate_write(GuestPtr(0x260), 1)?;
        super::super::set_volume(m, 7)?;
        self.output_gain = gain;
        Ok(0)
    }
}
