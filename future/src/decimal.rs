//! Roman-script integer conversions; no Script Manager patches or SANE selectors.
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Format {
        number: i32,
        destination: GuestPtr,
    },
    Parse {
        source: GuestPtr,
        destination: Option<GuestPtr>,
    },
}
pub fn call(memory: &mut Memory, op: Op) -> Result<u32> {
    match op {
        Op::Format {
            number,
            destination,
        } => {
            let text = number.to_string();
            let mut bytes = Vec::with_capacity(text.len() + 1);
            bytes.push(text.len() as u8);
            bytes.extend_from_slice(text.as_bytes());
            memory.write(destination, &bytes)?;
            Ok(0)
        }
        Op::Parse {
            source,
            destination,
        } => {
            if let Some(p) = destination {
                memory.validate_write(p, 4)?;
            }
            let len = usize::from(memory.read(source, 1)?[0]);
            // Snapshot the complete Pascal string before an overlapping output write.
            let bytes = memory.read(source, len + 1)?;
            let text = &bytes[1..];
            let negative = text.first() == Some(&b'-');
            let digits = if matches!(text.first(), Some(b'+' | b'-')) {
                &text[1..]
            } else {
                text
            };
            let mut value = 0u32;
            for &digit in digits {
                value = value.wrapping_mul(10).wrapping_add(u32::from(digit & 15));
            }
            if negative {
                value = value.wrapping_neg();
            }
            if let Some(p) = destination {
                memory.write(p, &value.to_be_bytes())?;
            }
            Ok(value)
        }
    }
}
