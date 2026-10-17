//! Version-2 Alias Manager wire layout, separate from catalog policy.
//! Fixed fields compared with Apple's xnu bsd/hfs/hfs_link.c aliasrec;
//! classic parent-name/full-path tags compared with preserved Systemless.
#[derive(Debug, Clone)]
pub(super) struct Record {
    pub header: [u8; 150],
    pub parent_name: Option<Vec<u8>>,
    pub path: Option<Vec<u8>>,
    extras: Vec<(i16, Vec<u8>)>,
}
impl Record {
    pub fn fresh() -> Self {
        let mut header = [0; 150];
        header[6..8].copy_from_slice(&2u16.to_be_bytes());
        header[42..44].copy_from_slice(&0x4244u16.to_be_bytes());
        header[130..134].fill(255);
        Self {
            header,
            parent_name: None,
            path: None,
            extras: Vec::new(),
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, i16> {
        if bytes.len() < 150 {
            return Err(-50);
        }
        let size = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
        if size < 150
            || size > bytes.len()
            || bytes[6..8] != [0, 2]
            || !matches!(u16::from_be_bytes(bytes[8..10].try_into().unwrap()), 0 | 1)
            || bytes[10] > 27
            || bytes[50] > 63
        {
            return Err(-50);
        }
        let mut out = Self {
            header: bytes[..150].try_into().unwrap(),
            parent_name: None,
            path: None,
            extras: Vec::new(),
        };
        let mut at = 150;
        while at < size {
            let head = bytes
                .get(at..at + 4)
                .filter(|_| at + 4 <= size)
                .ok_or(-50i16)?;
            let tag = i16::from_be_bytes(head[..2].try_into().unwrap());
            let n = usize::from(u16::from_be_bytes(head[2..].try_into().unwrap()));
            at += 4;
            if tag == -1 {
                if n != 0 || at != size {
                    return Err(-50);
                }
                return Ok(out);
            }
            let end = at.checked_add(n).filter(|&v| v <= size).ok_or(-50i16)?;
            let value = bytes[at..end].to_vec();
            match tag {
                0 if out.parent_name.is_none() => out.parent_name = Some(value),
                2 if out.path.is_none() => out.path = Some(value),
                0 | 2 => return Err(-50),
                _ => out.extras.push((tag, value)),
            }
            at = (end + 1) & !1;
            if at > size {
                return Err(-50);
            }
        }
        if size == 150 {
            Ok(out)
        } else {
            Err(-50)
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, i16> {
        let mut out = self.header.to_vec();
        let mut append = |tag: i16, data: &[u8]| -> Result<(), i16> {
            let n = u16::try_from(data.len()).map_err(|_| -50i16)?;
            out.extend(tag.to_be_bytes());
            out.extend(n.to_be_bytes());
            out.extend(data);
            if !out.len().is_multiple_of(2) {
                out.push(0);
            }
            Ok(())
        };
        if let Some(v) = &self.parent_name {
            append(0, v)?;
        }
        if let Some(v) = &self.path {
            append(2, v)?;
        }
        for (tag, value) in &self.extras {
            append(*tag, value)?;
        }
        append(-1, &[])?;
        let size = u16::try_from(out.len()).map_err(|_| -50i16)?;
        out[4..6].copy_from_slice(&size.to_be_bytes());
        Ok(out)
    }
    pub fn preserve_private(&mut self, old: &Self) {
        self.header[..4].copy_from_slice(&old.header[..4]);
        self.header[134..].copy_from_slice(&old.header[134..]);
        self.extras = old.extras.clone();
    }
    pub fn word(&self, offset: usize) -> u16 {
        u16::from_be_bytes(self.header[offset..offset + 2].try_into().unwrap())
    }
    pub fn long(&self, offset: usize) -> u32 {
        u32::from_be_bytes(self.header[offset..offset + 4].try_into().unwrap())
    }
    pub fn string(&self, offset: usize) -> &[u8] {
        &self.header[offset + 1..offset + 1 + usize::from(self.header[offset])]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_fields_tag_padding_end_marker_and_handle_slack_are_checked() {
        let mut r = Record::fresh();
        r.header[50..54].copy_from_slice(b"\x03abc");
        r.parent_name = Some(b"Odd".to_vec());
        r.path = Some(b"Volume:Odd:abc".to_vec());
        let mut b = r.encode().unwrap();
        assert_eq!(&b[6..10], &[0, 2, 0, 0]);
        assert_eq!(&b[42..44], b"BD");
        assert_eq!(&b[150..158], b"\0\0\0\x03Odd\0");
        assert_eq!(&b[b.len() - 4..], &[255, 255, 0, 0]);
        assert_eq!(
            usize::from(u16::from_be_bytes(b[4..6].try_into().unwrap())),
            b.len()
        );
        let parsed = Record::parse(&b).unwrap();
        assert_eq!(parsed.path, r.path);
        b.extend([0xa5; 8]);
        assert!(Record::parse(&b).is_ok());
        b[6] = 1;
        assert!(Record::parse(&b).is_err());
        b[6] = 0;
        b[10] = 28;
        assert!(Record::parse(&b).is_err());
        let mut b = r.encode().unwrap();
        let end = b.len();
        b[end - 1] = 1;
        assert!(Record::parse(&b).is_err());
        b.truncate(153);
        assert!(Record::parse(&b).is_err());
    }
}
