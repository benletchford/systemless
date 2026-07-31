//! Finder-compatible application icon-family discovery and decoding.
//!
//! Classic applications associate their `'APPL'` file reference with an
//! `'ICN#'` family through a `'BNDL'` resource. Color and small variants share
//! that `'ICN#'` resource ID. Macintosh Toolbox Essentials 1992, pp. 7-19..7-23
//! and 7-57..7-67.

use crate::managers::resource::ResourceFork;
use crate::runner::FixtureRunner;

const CUSTOM_ICON_ID: i16 = -16455;
const HAS_CUSTOM_ICON: u16 = 0x0400;

/// One decoded member of a classic Finder icon family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationIconRepresentation {
    pub width: u16,
    pub height: u16,
    /// Straight-alpha RGBA pixels in row-major order.
    pub rgba: Vec<u8>,
}

/// The decoded icon family belonging to the launched classic application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationIcon {
    pub representations: Vec<ApplicationIconRepresentation>,
}

/// Host-facing identity of the application selected inside an archive or disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationIdentity {
    pub path: String,
    pub name: String,
    pub icon: Option<ApplicationIcon>,
}

/// Resolve the identity of the runner's current foreground application.
///
/// This intentionally derives the icon from the selected executable's own
/// resource fork rather than from the outer StuffIt, MacBinary, or disk-image
/// filename.
pub fn loaded_application_identity(runner: &FixtureRunner) -> Option<ApplicationIdentity> {
    let dispatcher = runner.dispatcher();
    let path = dispatcher.launched_app_path()?.to_owned();
    let name = path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path.as_str())
        .to_owned();
    let resource_key = dispatcher.find_vfs_rsrc_file(&path);
    let metadata = resource_key
        .as_ref()
        .and_then(|key| dispatcher.vfs_metadata.get(key))
        .or_else(|| dispatcher.vfs_metadata.get(&path))
        .copied();
    let creator = metadata
        .map(|metadata| metadata.creator.to_be_bytes())
        .unwrap_or(*b"????");
    let finder_flags = metadata.map_or(0, |metadata| metadata.finder_flags);
    let icon = resource_key
        .as_ref()
        .and_then(|key| dispatcher.vfs_rsrc.get(key))
        .and_then(|bytes| ResourceFork::parse(bytes))
        .and_then(|fork| application_icon_from_fork(&fork, creator, finder_flags));

    Some(ApplicationIdentity { path, name, icon })
}

/// Decode the Finder icon family associated with an application resource fork.
pub fn application_icon_from_fork(
    fork: &ResourceFork,
    creator: [u8; 4],
    finder_flags: u16,
) -> Option<ApplicationIcon> {
    let icon_id =
        if finder_flags & HAS_CUSTOM_ICON != 0 && fork.get(*b"ICN#", CUSTOM_ICON_ID).is_some() {
            CUSTOM_ICON_ID
        } else {
            bundled_application_icon_id(fork, creator)?
        };

    let mut representations = Vec::with_capacity(2);
    if let Some(icon) = decode_icon_size(fork, icon_id, 32) {
        representations.push(icon);
    }
    if let Some(icon) = decode_icon_size(fork, icon_id, 16) {
        representations.push(icon);
    }
    (!representations.is_empty()).then_some(ApplicationIcon { representations })
}

fn bundled_application_icon_id(fork: &ResourceFork, creator: [u8; 4]) -> Option<i16> {
    let mut bundles = fork.get_all(*b"BNDL");
    bundles.sort_by_key(|resource| resource.id);
    let matching = bundles
        .iter()
        .copied()
        .filter(|resource| resource.data.get(..4) == Some(creator.as_slice()))
        .collect::<Vec<_>>();
    let bundle = if let [bundle, ..] = matching.as_slice() {
        *bundle
    } else if let [bundle] = bundles.as_slice() {
        *bundle
    } else {
        return None;
    };

    let mappings = parse_bundle_mappings(&bundle.data)?;
    let fref_ids = mappings
        .iter()
        .find(|mapping| mapping.resource_type == *b"FREF")?;
    let application_local_id = fref_ids.pairs.iter().find_map(|&(_, resource_id)| {
        let fref = fork.get(*b"FREF", resource_id)?;
        if fref.data.get(..4) != Some(b"APPL") {
            return None;
        }
        read_i16(&fref.data, 4)
    })?;
    mappings
        .iter()
        .find(|mapping| mapping.resource_type == *b"ICN#")?
        .pairs
        .iter()
        .find_map(|&(local_id, resource_id)| {
            (local_id == application_local_id).then_some(resource_id)
        })
}

#[derive(Debug)]
struct BundleMapping {
    resource_type: [u8; 4],
    pairs: Vec<(i16, i16)>,
}

fn parse_bundle_mappings(data: &[u8]) -> Option<Vec<BundleMapping>> {
    // The compiled counts are stored as "number minus one", matching classic
    // Resource Manager array conventions. Macintosh Toolbox Essentials 1992,
    // pp. 7-66..7-67.
    let type_count = usize::from(read_u16(data, 6)?).checked_add(1)?;
    let mut offset = 8usize;
    let mut mappings = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let resource_type: [u8; 4] = data.get(offset..offset + 4)?.try_into().ok()?;
        let pair_count = usize::from(read_u16(data, offset + 4)?).checked_add(1)?;
        offset = offset.checked_add(6)?;
        let pairs_bytes = pair_count.checked_mul(4)?;
        let end = offset.checked_add(pairs_bytes)?;
        if end > data.len() {
            return None;
        }
        let mut pairs = Vec::with_capacity(pair_count);
        for pair in data[offset..end].chunks_exact(4) {
            pairs.push((
                i16::from_be_bytes([pair[0], pair[1]]),
                i16::from_be_bytes([pair[2], pair[3]]),
            ));
        }
        mappings.push(BundleMapping {
            resource_type,
            pairs,
        });
        offset = end;
    }
    Some(mappings)
}

fn decode_icon_size(
    fork: &ResourceFork,
    icon_id: i16,
    size: usize,
) -> Option<ApplicationIconRepresentation> {
    let (mask_type, color8_type, color4_type, bitmap_len) = match size {
        32 => (*b"ICN#", *b"icl8", *b"icl4", 128usize),
        16 => (*b"ics#", *b"ics8", *b"ics4", 32usize),
        _ => return None,
    };
    let mask_resource = fork.get(mask_type, icon_id)?;
    let mask_offset = bitmap_len;
    let mask = mask_resource
        .data
        .get(mask_offset..mask_offset.checked_add(bitmap_len)?)?;
    let rgba = if let Some(color) = fork.get(color8_type, icon_id) {
        decode_indexed(&color.data, mask, size, 8)
    } else if let Some(color) = fork.get(color4_type, icon_id) {
        decode_indexed(&color.data, mask, size, 4)
    } else {
        let bitmap = mask_resource.data.get(..bitmap_len)?;
        decode_monochrome(bitmap, mask, size)
    }?;
    Some(ApplicationIconRepresentation {
        width: size as u16,
        height: size as u16,
        rgba,
    })
}

fn decode_indexed(data: &[u8], mask: &[u8], size: usize, depth: u16) -> Option<Vec<u8>> {
    let row_bytes = size.checked_mul(depth as usize)?.checked_div(8)?;
    let required = row_bytes.checked_mul(size)?;
    if data.len() < required || mask.len() < size.checked_mul(size)?.checked_div(8)? {
        return None;
    }
    let (palette, _) = crate::trap::TrapDispatcher::standard_mac_indexed_clut(depth)?;
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let index = match depth {
                8 => data[y * row_bytes + x] as usize,
                4 => {
                    let byte = data[y * row_bytes + x / 2];
                    if x & 1 == 0 {
                        (byte >> 4) as usize
                    } else {
                        (byte & 0x0F) as usize
                    }
                }
                _ => return None,
            };
            let [red, green, blue] = palette[index];
            rgba.extend_from_slice(&[
                (red >> 8) as u8,
                (green >> 8) as u8,
                (blue >> 8) as u8,
                mask_alpha(mask, size, x, y)?,
            ]);
        }
    }
    Some(rgba)
}

fn decode_monochrome(bitmap: &[u8], mask: &[u8], size: usize) -> Option<Vec<u8>> {
    let required = size.checked_mul(size)?.checked_div(8)?;
    if bitmap.len() < required || mask.len() < required {
        return None;
    }
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let byte = bitmap[y * (size / 8) + x / 8];
            let black = byte & (0x80 >> (x & 7)) != 0;
            let component = if black { 0 } else { 255 };
            rgba.extend_from_slice(&[
                component,
                component,
                component,
                mask_alpha(mask, size, x, y)?,
            ]);
        }
    }
    Some(rgba)
}

fn mask_alpha(mask: &[u8], size: usize, x: usize, y: usize) -> Option<u8> {
    let byte = *mask.get(y.checked_mul(size / 8)?.checked_add(x / 8)?)?;
    Some(if byte & (0x80 >> (x & 7)) != 0 {
        255
    } else {
        0
    })
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *data.get(offset)?,
        *data.get(offset.checked_add(1)?)?,
    ]))
}

fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    read_u16(data, offset).map(|value| value as i16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(resource_type: [u8; 4], id: i16, data: Vec<u8>) -> ([u8; 4], i16, Vec<u8>) {
        (resource_type, id, data)
    }

    fn bundle(signature: [u8; 4], application_icon_id: i16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&signature);
        data.extend_from_slice(&0i16.to_be_bytes());
        data.extend_from_slice(&1u16.to_be_bytes()); // two mapping types minus one
        data.extend_from_slice(b"ICN#");
        data.extend_from_slice(&0u16.to_be_bytes()); // one pair minus one
        data.extend_from_slice(&7i16.to_be_bytes());
        data.extend_from_slice(&application_icon_id.to_be_bytes());
        data.extend_from_slice(b"FREF");
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend_from_slice(&0i16.to_be_bytes());
        data.extend_from_slice(&208i16.to_be_bytes());
        data
    }

    fn fref(file_type: [u8; 4], local_id: i16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&file_type);
        data.extend_from_slice(&local_id.to_be_bytes());
        data.push(0);
        data
    }

    fn icon_list(size: usize, bitmap_fill: u8, mask_fill: u8) -> Vec<u8> {
        let bytes = size * size / 8;
        let mut data = vec![bitmap_fill; bytes];
        data.extend(std::iter::repeat_n(mask_fill, bytes));
        data
    }

    #[test]
    fn bundle_resolves_appl_fref_local_id_to_icon_family() {
        let fork = ResourceFork::from_test_resources(vec![
            resource(*b"BNDL", 128, bundle(*b"TEST", 321)),
            resource(*b"FREF", 208, fref(*b"APPL", 7)),
            resource(*b"ICN#", 321, icon_list(32, 0xFF, 0xFF)),
        ]);
        let icon = application_icon_from_fork(&fork, *b"TEST", 0).unwrap();
        assert_eq!(icon.representations.len(), 1);
        assert_eq!(icon.representations[0].width, 32);
        assert_eq!(&icon.representations[0].rgba[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn creator_selects_the_matching_bundle_when_multiple_are_present() {
        let fork = ResourceFork::from_test_resources(vec![
            resource(*b"BNDL", 128, bundle(*b"OTHR", 320)),
            resource(*b"BNDL", 129, bundle(*b"TEST", 321)),
            resource(*b"FREF", 208, fref(*b"APPL", 7)),
            resource(*b"ICN#", 320, icon_list(32, 0x00, 0xFF)),
            resource(*b"ICN#", 321, icon_list(32, 0xFF, 0xFF)),
        ]);
        let icon = application_icon_from_fork(&fork, *b"TEST", 0).unwrap();
        assert_eq!(&icon.representations[0].rgba[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn color_icon_uses_mask_for_transparency_and_prefers_eight_bit_pixels() {
        let mut mask = icon_list(32, 0, 0);
        mask[128] = 0x80;
        let mut color = vec![255; 32 * 32];
        color[0] = 0;
        let fork = ResourceFork::from_test_resources(vec![
            resource(*b"BNDL", 128, bundle(*b"TEST", 321)),
            resource(*b"FREF", 208, fref(*b"APPL", 7)),
            resource(*b"ICN#", 321, mask),
            resource(*b"icl8", 321, color),
            resource(*b"icl4", 321, vec![0xFF; 32 * 32 / 2]),
        ]);
        let icon = application_icon_from_fork(&fork, *b"TEST", 0).unwrap();
        assert_eq!(&icon.representations[0].rgba[..4], &[255, 255, 255, 255]);
        assert_eq!(&icon.representations[0].rgba[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn custom_icon_flag_overrides_bundle_family() {
        let fork = ResourceFork::from_test_resources(vec![
            resource(*b"BNDL", 128, bundle(*b"TEST", 321)),
            resource(*b"FREF", 208, fref(*b"APPL", 7)),
            resource(*b"ICN#", 321, icon_list(32, 0x00, 0xFF)),
            resource(*b"ICN#", CUSTOM_ICON_ID, icon_list(32, 0xFF, 0xFF)),
        ]);
        let icon = application_icon_from_fork(&fork, *b"TEST", HAS_CUSTOM_ICON).unwrap();
        assert_eq!(&icon.representations[0].rgba[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn four_bit_large_and_small_variants_decode_from_the_system_palette() {
        let mut large = vec![0; 32 * 32 / 2];
        large[0] = 0x1F;
        let mut small = vec![0; 16 * 16 / 2];
        small[0] = 0x1F;
        let fork = ResourceFork::from_test_resources(vec![
            resource(*b"BNDL", 128, bundle(*b"TEST", 321)),
            resource(*b"FREF", 208, fref(*b"APPL", 7)),
            resource(*b"ICN#", 321, icon_list(32, 0, 0xFF)),
            resource(*b"icl4", 321, large),
            resource(*b"ics#", 321, icon_list(16, 0, 0xFF)),
            resource(*b"ics4", 321, small),
        ]);
        let icon = application_icon_from_fork(&fork, *b"TEST", 0).unwrap();
        assert_eq!(
            icon.representations
                .iter()
                .map(|representation| representation.width)
                .collect::<Vec<_>>(),
            vec![32, 16]
        );
        for representation in &icon.representations {
            assert_eq!(&representation.rgba[..4], &[0xFC, 0xF3, 0x05, 0xFF]);
            assert_eq!(&representation.rgba[4..8], &[0, 0, 0, 0xFF]);
        }
    }

    #[test]
    fn malformed_bundle_and_truncated_icon_fail_without_panicking() {
        let malformed_bundle =
            ResourceFork::from_test_resources(vec![resource(*b"BNDL", 128, vec![0; 9])]);
        assert_eq!(
            application_icon_from_fork(&malformed_bundle, *b"TEST", 0),
            None
        );

        let truncated = ResourceFork::from_test_resources(vec![
            resource(*b"BNDL", 128, bundle(*b"TEST", 321)),
            resource(*b"FREF", 208, fref(*b"APPL", 7)),
            resource(*b"ICN#", 321, vec![0; 20]),
        ]);
        assert_eq!(application_icon_from_fork(&truncated, *b"TEST", 0), None);
    }
}
