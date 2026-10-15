//! Guest QuickDraw rectangle metadata, shared by both ABI adapters.
use crate::{
    memory::{GuestPtr, Memory},
    Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Set { pointer: GuestPtr, edges: [i16; 4] },
    Offset { pointer: GuestPtr, dh: i16, dv: i16 },
    Inset { pointer: GuestPtr, dh: i16, dv: i16 },
}
pub fn call(memory: &mut Memory, op: Op) -> Result<()> {
    let (pointer, values) = match op {
        Op::Set { pointer, edges } => (pointer, edges),
        Op::Offset { pointer, dh, dv } | Op::Inset { pointer, dh, dv } => {
            let bytes = memory.read(pointer, 8)?;
            let mut values: [i16; 4] = std::array::from_fn(|i| {
                i16::from_be_bytes(bytes[i * 2..i * 2 + 2].try_into().unwrap())
            });
            values[0] = values[0].wrapping_add(dv);
            values[1] = values[1].wrapping_add(dh);
            if matches!(op, Op::Inset { .. }) {
                values[2] = values[2].wrapping_sub(dv);
                values[3] = values[3].wrapping_sub(dh);
            } else {
                values[2] = values[2].wrapping_add(dv);
                values[3] = values[3].wrapping_add(dh);
            }
            (pointer, values)
        }
    };
    memory.write(
        pointer,
        &values
            .into_iter()
            .flat_map(i16::to_be_bytes)
            .collect::<Vec<_>>(),
    )
}

/// Pixel below/right of the coordinate: top/left included, bottom/right excluded.
pub fn contains(memory: &Memory, point: [i16; 2], rectangle: GuestPtr) -> Result<bool> {
    let rect = crate::quickdraw::read_rect(memory, rectangle)?;
    Ok(i32::from(point[0]) >= rect.top
        && i32::from(point[0]) < rect.bottom
        && i32::from(point[1]) >= rect.left
        && i32::from(point[1]) < rect.right)
}
