//! PEF relocations, Mac OS Runtime Architectures (1997), pp. 8-23–8-36.
//! Relocations ADD to existing words, including imports and indexed sections.
use super::{invalid, u16_at, u32_at};
use crate::Result;
#[derive(Clone, Copy)]
enum Op {
    Skip(u32, u32),
    Run(u16, u32),
    Index(u16, u32),
    Advance(u32),
    Position(u32),
    Repeat(usize, u32),
}
struct Instruction {
    start: usize,
    op: Op,
}
pub(super) fn apply(
    data: &mut [u8],
    stream: &[u8],
    deltas: &[u32],
    imports: &[u32],
    budget: &mut usize,
) -> Result<()> {
    if !stream.len().is_multiple_of(2) {
        return Err(invalid("odd relocation stream"));
    }
    let mut instructions = Vec::new();
    let mut offset = 0;
    while offset < stream.len() {
        let start = offset;
        let word = u16_at(stream, offset)?;
        offset += 2;
        let dispatch = word >> 9;
        let small = u32::from(word & 0x1ff);
        let op = match dispatch {
            0..=0x1f => Op::Skip(u32::from((word >> 6) & 255) * 4, u32::from(word & 63)),
            0x20..=0x25 => Op::Run(dispatch, small + 1),
            0x30..=0x33 => Op::Index(dispatch, small),
            0x40..=0x47 => Op::Advance(u32::from(word & 0xfff) + 1),
            0x48..=0x4f => Op::Repeat(usize::from((word >> 8) & 15) + 1, u32::from(word & 255) + 1),
            0x50..=0x53 | 0x58..=0x5b => {
                let low = u32::from(u16_at(stream, offset)?);
                offset += 2;
                match dispatch {
                    0x50 | 0x51 => Op::Position((u32::from(word & 0x3ff) << 16) | low),
                    0x52 | 0x53 => Op::Index(0x30, (u32::from(word & 0x3ff) << 16) | low),
                    0x58 | 0x59 => Op::Repeat(
                        usize::from((word >> 6) & 15) + 1,
                        (u32::from(word & 63) << 16) | low,
                    ),
                    _ => {
                        let kind = match (word >> 6) & 15 {
                            0 => 0x33,
                            1 => 0x31,
                            2 => 0x32,
                            _ => return Err(invalid("unknown long section relocation")),
                        };
                        Op::Index(kind, (u32::from(word & 63) << 16) | low)
                    }
                }
            }
            _ => return Err(invalid(format!("unsupported PEF relocation {word:#06x}"))),
        };
        instructions.push(Instruction { start, op });
    }
    let mut state = State {
        data,
        deltas,
        imports,
        position: 0,
        import: 0,
        c: deltas.first().copied().unwrap_or(0),
        d: deltas.get(1).copied().unwrap_or(0),
        budget,
    };
    for (i, instruction) in instructions.iter().enumerate() {
        match instruction.op {
            Op::Repeat(chunks, count) => {
                let start = instruction
                    .start
                    .checked_sub(chunks * 2)
                    .ok_or_else(|| invalid("relocation repeat underflow"))?;
                let first = instructions[..i]
                    .iter()
                    .position(|op| op.start == start)
                    .ok_or_else(|| invalid("repeat begins inside relocation"))?;
                let repeated = &instructions[first..i];
                if repeated.iter().any(|i| matches!(i.op, Op::Repeat(..))) {
                    return Err(invalid("nested PEF relocation repeat"));
                }
                state.charge()?;
                for _ in 0..count {
                    for op in repeated {
                        state.one(op.op)?;
                    }
                }
            }
            op => state.one(op)?,
        }
    }
    Ok(())
}
struct State<'a> {
    data: &'a mut [u8],
    deltas: &'a [u32],
    imports: &'a [u32],
    position: usize,
    import: usize,
    c: u32,
    d: u32,
    budget: &'a mut usize,
}
impl State<'_> {
    fn charge(&mut self) -> Result<()> {
        *self.budget = self
            .budget
            .checked_sub(1)
            .ok_or_else(|| invalid("relocation work budget exceeded"))?;
        Ok(())
    }
    fn advance(&mut self, n: usize) -> Result<()> {
        self.position = self
            .position
            .checked_add(n)
            .filter(|v| *v <= self.data.len())
            .ok_or_else(|| invalid("relocation position outside section"))?;
        Ok(())
    }
    fn add(&mut self, value: u32) -> Result<()> {
        self.charge()?;
        if !self.position.is_multiple_of(4) {
            return Err(invalid("unaligned relocation slot"));
        }
        let value = u32_at(self.data, self.position)?.wrapping_add(value);
        self.data[self.position..self.position + 4].copy_from_slice(&value.to_be_bytes());
        self.advance(4)
    }
    fn import(&mut self) -> Result<()> {
        let value = *self
            .imports
            .get(self.import)
            .ok_or_else(|| invalid("relocation import index"))?;
        self.add(value)?;
        self.import += 1;
        Ok(())
    }
    fn one(&mut self, op: Op) -> Result<()> {
        self.charge()?;
        match op {
            Op::Skip(skip, count) => {
                self.advance(skip as usize)?;
                for _ in 0..count {
                    self.add(self.d)?;
                }
            }
            Op::Run(kind, count) => {
                for _ in 0..count {
                    match kind {
                        0x20 => self.add(self.c)?,
                        0x21 => self.add(self.d)?,
                        0x22 | 0x23 => {
                            self.add(self.c)?;
                            self.add(self.d)?;
                            if kind == 0x22 {
                                self.advance(4)?;
                            }
                        }
                        0x24 => {
                            self.add(self.d)?;
                            self.advance(4)?;
                        }
                        0x25 => self.import()?,
                        _ => unreachable!(),
                    }
                }
            }
            Op::Index(kind, index) => {
                if kind == 0x30 {
                    self.import = index as usize;
                    self.import()?;
                } else {
                    let value = *self
                        .deltas
                        .get(index as usize)
                        .ok_or_else(|| invalid("relocation section index"))?;
                    match kind {
                        0x31 => self.c = value,
                        0x32 => self.d = value,
                        0x33 => self.add(value)?,
                        _ => unreachable!(),
                    }
                }
            }
            Op::Advance(n) => self.advance(n as usize)?,
            Op::Position(n) => {
                self.position = 0;
                self.advance(n as usize)?;
            }
            Op::Repeat(..) => unreachable!(),
        };
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(words: &[u16], values: &[u32], deltas: &[u32], imports: &[u32]) -> Result<Vec<u32>> {
        let mut data: Vec<_> = values.iter().flat_map(|v| v.to_be_bytes()).collect();
        let stream: Vec<_> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
        apply(&mut data, &stream, deltas, imports, &mut 10000)?;
        Ok(data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_be_bytes(*w))
            .collect())
    }
    #[test]
    fn relocate_values_and_addends() {
        assert_eq!(
            run(&[0x4001, 0x4200], &[1, 2, 3], &[100, 200], &[]).unwrap(),
            [101, 102, 203]
        );
        assert_eq!(
            run(
                &[0x4600, 0x4400, 0x4800],
                &[1, 2, 3, 4, 5, 6, 7],
                &[100, 200],
                &[]
            )
            .unwrap(),
            [101, 202, 103, 204, 5, 206, 7]
        );
        assert_eq!(run(&[0x0041], &[1, 2], &[100, 200], &[]).unwrap(), [1, 202]);
        assert_eq!(
            run(&[0x4a01], &[3, 4], &[], &[100, 200]).unwrap(),
            [103, 204]
        );
    }
    #[test]
    fn indexed_small_and_large_sections_and_imports() {
        assert_eq!(
            run(
                &[0x6202, 0x6400, 0x4000, 0x4200, 0x6601, 0x6001],
                &[1, 2, 3, 4],
                &[100, 200, 300],
                &[10, 20]
            )
            .unwrap(),
            [301, 102, 203, 24]
        );
        assert_eq!(
            run(
                &[0xb440, 2, 0xb480, 0, 0x4600, 0xb400, 1, 0xa400, 1],
                &[1, 2, 3, 4],
                &[100, 200, 300],
                &[10, 20]
            )
            .unwrap(),
            [301, 102, 203, 24]
        );
    }
    #[test]
    fn repeats_and_position_preserve_state() {
        assert_eq!(
            run(&[0x4000, 0x9001], &[1, 2, 3], &[100], &[]).unwrap(),
            [101, 102, 103]
        );
        assert_eq!(
            run(&[0x4000, 0xb000, 2], &[1, 2, 3], &[100], &[]).unwrap(),
            [101, 102, 103]
        );
        assert_eq!(
            run(
                &[0x8003, 0x4000, 0xa000, 0, 0x4200],
                &[1, 2],
                &[100, 200],
                &[]
            )
            .unwrap(),
            [201, 102]
        );
    }
    #[test]
    fn malformed_relocations_are_bounded() {
        for stream in [
            vec![0x9000],
            vec![0xa000],
            vec![0x4000, 0x9000, 0x9100],
            vec![0x8000, 0x4000],
            vec![0x6000],
            vec![0x6207],
            vec![0xffff],
            vec![0xa000, 0, 0x9000],
        ] {
            assert!(run(&stream, &[0; 16], &[1], &[]).is_err(), "{stream:x?}");
        }
        let mut data = [0; 64];
        let stream = [0x40, 0, 0xb0, 0x3f, 0xff, 0xff];
        assert!(apply(&mut data, &stream, &[1], &[], &mut 5).is_err());
    }
}
