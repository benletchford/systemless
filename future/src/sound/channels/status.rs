//! Live status for the virtual Sound Manager 3.0 profile. Disk playback and
//! pause commands are unsupported, so disk times/paused remain zero. CPU-load
//! fields are obsolete in 3.0 and deliberately contain no fabricated estimate.
use super::*;
impl Manager {
    pub fn status(
        &self,
        m: &mut Memory,
        p: &mut Process,
        request: crate::sound::Status,
    ) -> Result<i16> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            let size = if request.channel.is_some() { 24 } else { 6 };
            if request.length < i16::try_from(size).unwrap() || request.destination.0 == 0 {
                return Ok(-50);
            }
            let mut bytes = vec![0; size];
            if let Some(channel) = request.channel {
                if !self.valid(m, p, channel)? {
                    return Ok(BAD_CHANNEL);
                }
                bytes[12] = u8::from(self.busy(channel).unwrap());
                bytes[16..20].copy_from_slice(&self.channels[&channel].init.to_be_bytes());
            } else {
                for &channel in &self.order {
                    if !self.valid(m, p, channel)? {
                        return Ok(BAD_CHANNEL);
                    }
                }
                bytes[2..4].copy_from_slice(&(self.channels.len() as u16).to_be_bytes());
            }
            m.write(request.destination, &bytes)?;
            Ok(0)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
