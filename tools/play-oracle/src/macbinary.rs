//! MacBinary III file format encoding.
//!
//! Converts AppleDouble format (data fork + resource fork + Finder info)
//! to MacBinary III format for use with classic Mac emulators.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Create MacBinary III file from AppleDouble format files
pub fn create_macbinary(output_dir: &Path) -> std::io::Result<()> {
    let data_path = output_dir.join("FixtureGen");
    let rsrc_path = output_dir.join(".rsrc/FixtureGen");
    let finf_path = output_dir.join(".finf/FixtureGen");
    let output_path = output_dir.join("FixtureGen.bin");

    let data_fork = fs::read(&data_path)?;
    let rsrc_fork = fs::read(&rsrc_path)?;
    let finf = fs::read(&finf_path).unwrap_or_else(|_| vec![0u8; 32]);

    let file_type: [u8; 4] = if finf.len() >= 4 {
        finf[0..4].try_into().unwrap()
    } else {
        *b"APPL"
    };
    let creator: [u8; 4] = if finf.len() >= 8 {
        finf[4..8].try_into().unwrap()
    } else {
        *b"????"
    };

    let filename = b"FixtureGen";
    let mut output = fs::File::create(&output_path)?;

    encode_macbinary(
        &mut output,
        filename,
        &file_type,
        &creator,
        &data_fork,
        &rsrc_fork,
    )?;

    Ok(())
}

/// Encode raw data into MacBinary III format
pub fn encode_macbinary<W: Write>(
    writer: &mut W,
    filename: &[u8],
    file_type: &[u8; 4],
    creator: &[u8; 4],
    data_fork: &[u8],
    rsrc_fork: &[u8],
) -> std::io::Result<()> {
    let mut header = [0u8; 128];
    header[0] = 0;

    let name_len = filename.len().min(63);
    header[1] = name_len as u8;
    header[2..2 + name_len].copy_from_slice(&filename[..name_len]);

    header[65..69].copy_from_slice(file_type);
    header[69..73].copy_from_slice(creator);

    let data_len = data_fork.len() as u32;
    header[83..87].copy_from_slice(&data_len.to_be_bytes());

    let rsrc_len = rsrc_fork.len() as u32;
    header[87..91].copy_from_slice(&rsrc_len.to_be_bytes());

    header[102..106].copy_from_slice(b"mBIN");
    header[106] = 0;

    let total_len = (data_fork.len() + rsrc_fork.len()) as u32;
    header[116..120].copy_from_slice(&total_len.to_be_bytes());

    header[120] = 0;
    header[121] = 0;
    header[122] = 130;
    header[123] = 129;

    let crc = crc16_ccitt(&header[0..124]);
    header[124..126].copy_from_slice(&crc.to_be_bytes());

    writer.write_all(&header)?;

    writer.write_all(data_fork)?;
    let data_padding = (128 - (data_fork.len() % 128)) % 128;
    writer.write_all(&vec![0u8; data_padding])?;

    writer.write_all(rsrc_fork)?;
    let rsrc_padding = (128 - (rsrc_fork.len() % 128)) % 128;
    writer.write_all(&vec![0u8; rsrc_padding])?;

    Ok(())
}

/// Calculate CRC-16-CCITT checksum
fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}
