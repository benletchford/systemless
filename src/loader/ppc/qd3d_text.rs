//! Text 3DMF input for the existing QuickDraw 3D binary object reader.
//!
//! Apple, *QuickDraw 3D Metafile Reference*, describes text and binary 3DMF
//! as two serializations of the same objects. Keep unsupported text objects
//! explicit so a partial conversion cannot silently change a scene.

use super::{
    PPC_Q3_PIXEL_TYPE_ARGB16, PPC_Q3_PIXEL_TYPE_ARGB32, PPC_Q3_PIXEL_TYPE_RGB16,
    PPC_Q3_PIXEL_TYPE_RGB16_565, PPC_Q3_PIXEL_TYPE_RGB24, PPC_Q3_PIXEL_TYPE_RGB32,
};

#[derive(Clone, Copy)]
enum Token<'a> {
    Word(&'a str),
    Open,
    Close,
}

struct Node<'a> {
    name: &'a str,
    values: Vec<&'a str>,
    children: Vec<Node<'a>>,
}

fn tokens(input: &str) -> Option<Vec<Token<'_>>> {
    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        match bytes[offset] {
            b'#' => {
                while offset < bytes.len() && bytes[offset] != b'\r' && bytes[offset] != b'\n' {
                    offset += 1;
                }
            }
            b'(' => {
                result.push(Token::Open);
                offset += 1;
            }
            b')' => {
                result.push(Token::Close);
                offset += 1;
            }
            byte if byte.is_ascii_whitespace() => offset += 1,
            _ => {
                let start = offset;
                while offset < bytes.len()
                    && !bytes[offset].is_ascii_whitespace()
                    && !matches!(bytes[offset], b'#' | b'(' | b')')
                {
                    offset += 1;
                }
                result.push(Token::Word(input.get(start..offset)?));
            }
        }
        if result.len() > 2_000_000 {
            return None;
        }
    }
    Some(result)
}

fn parse_node<'a>(tokens: &[Token<'a>], cursor: &mut usize, depth: usize) -> Option<Node<'a>> {
    if depth > 64 {
        return None;
    }
    let Token::Word(name) = *tokens.get(*cursor)? else {
        return None;
    };
    *cursor += 1;
    if !matches!(tokens.get(*cursor), Some(Token::Open)) {
        return None;
    }
    *cursor += 1;
    let mut node = Node {
        name,
        values: Vec::new(),
        children: Vec::new(),
    };
    loop {
        match *tokens.get(*cursor)? {
            Token::Close => {
                *cursor += 1;
                return Some(node);
            }
            Token::Open => return None,
            Token::Word(word) => {
                if word.ends_with(':') {
                    *cursor += 1;
                    node.children.push(parse_node(tokens, cursor, depth + 1)?);
                } else if matches!(tokens.get(*cursor + 1), Some(Token::Open)) {
                    node.children.push(parse_node(tokens, cursor, depth + 1)?);
                } else {
                    node.values.push(word);
                    *cursor += 1;
                }
            }
        }
    }
}

fn parse(input: &str) -> Option<Vec<Node<'_>>> {
    let tokens = tokens(input)?;
    let mut cursor = 0;
    let mut nodes = Vec::new();
    while cursor < tokens.len() {
        if let Some(Token::Word(label)) = tokens.get(cursor) {
            if label.ends_with(':') {
                cursor += 1;
            }
        }
        nodes.push(parse_node(&tokens, &mut cursor, 0)?);
    }
    Some(nodes)
}

fn integer(value: &str) -> Option<u32> {
    value.parse().ok()
}

fn float(value: &str) -> Option<u32> {
    let value: f32 = value.parse().ok()?;
    value.is_finite().then_some(value.to_bits())
}

fn boolean(value: &str) -> Option<u32> {
    match value {
        "False" => Some(0),
        "True" => Some(1),
        _ => None,
    }
}

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn chunk(kind: &[u8; 4], body: Vec<u8>) -> Option<Vec<u8>> {
    let size = u32::try_from(body.len()).ok()?;
    let mut result = Vec::with_capacity(body.len().checked_add(8)?);
    result.extend_from_slice(kind);
    word(&mut result, size);
    result.extend_from_slice(&body);
    Some(result)
}

fn next_u32(values: &[&str], cursor: &mut usize) -> Option<u32> {
    let result = integer(values.get(*cursor)?)?;
    *cursor += 1;
    Some(result)
}

fn next_float(values: &[&str], cursor: &mut usize) -> Option<u32> {
    let result = float(values.get(*cursor)?)?;
    *cursor += 1;
    Some(result)
}

fn index(bytes: &mut Vec<u8>, value: u32, count: u32) -> Option<()> {
    if value >= count {
        return None;
    }
    if count <= 0xff {
        bytes.push(u8::try_from(value).ok()?);
    } else if count <= 0xffff {
        bytes.extend_from_slice(&u16::try_from(value).ok()?.to_be_bytes());
    } else {
        word(bytes, value);
    }
    Some(())
}

fn trimesh_counts(node: &Node<'_>) -> Option<[u32; 6]> {
    if node.name != "TriMesh" {
        return None;
    }
    Some([
        integer(node.values.first()?)?,
        integer(node.values.get(1)?)?,
        integer(node.values.get(2)?)?,
        integer(node.values.get(3)?)?,
        integer(node.values.get(4)?)?,
        integer(node.values.get(5)?)?,
    ])
}

fn encode_trimesh(node: &Node<'_>) -> Option<Vec<u8>> {
    if !node.children.is_empty() {
        return None;
    }
    let counts = trimesh_counts(node)?;
    let [triangles, triangle_attributes, edges, edge_attributes, points, vertex_attributes] =
        counts;
    if triangles == 0
        || points == 0
        || triangle_attributes > 64
        || edge_attributes > 64
        || vertex_attributes > 64
        || triangles > 1_000_000
        || points > 1_000_000
    {
        return None;
    }
    let mut body = Vec::new();
    for count in counts {
        word(&mut body, count);
    }
    let mut cursor = 6;
    for _ in 0..triangles.checked_mul(3)? {
        index(&mut body, next_u32(&node.values, &mut cursor)?, points)?;
    }
    for _ in 0..edges {
        index(&mut body, next_u32(&node.values, &mut cursor)?, points)?;
        index(&mut body, next_u32(&node.values, &mut cursor)?, points)?;
        index(&mut body, next_u32(&node.values, &mut cursor)?, triangles)?;
        index(&mut body, next_u32(&node.values, &mut cursor)?, triangles)?;
    }
    for _ in 0..points.checked_mul(3)?.checked_add(6)? {
        word(&mut body, next_float(&node.values, &mut cursor)?);
    }
    word(&mut body, boolean(node.values.get(cursor)?)?);
    cursor += 1;
    (cursor == node.values.len()).then_some(body)
}

fn encode_attribute_array(node: &Node<'_>, counts: [u32; 6]) -> Option<Vec<u8>> {
    if !node.children.is_empty() || node.values.len() < 5 {
        return None;
    }
    let mut body = Vec::new();
    for value in node.values.iter().take(5) {
        word(&mut body, integer(value)?);
    }
    let attribute_type = integer(node.values[0])?;
    let which_array = integer(node.values[2])?;
    let use_array = integer(node.values[4])?;
    let elements = match which_array {
        0 => counts[0],
        1 => counts[2],
        2 => counts[4],
        _ => return None,
    };
    let components = match attribute_type {
        1 | 2 => 2,
        3 | 5 | 6 | 8 => 3,
        4 | 7 | 10 => 1,
        9 => 6,
        _ => return None,
    };
    let mut cursor = 5;
    if use_array != 0 {
        for _ in 0..elements {
            body.push(u8::try_from(next_u32(&node.values, &mut cursor)?).ok()?);
        }
    }
    for _ in 0..elements.checked_mul(components)? {
        word(&mut body, next_float(&node.values, &mut cursor)?);
    }
    (cursor == node.values.len())
        .then(|| body)
        .and_then(|body| chunk(b"atar", body))
}

fn pixel_type(value: &str) -> Option<u32> {
    match value {
        "RGB32" => Some(PPC_Q3_PIXEL_TYPE_RGB32),
        "ARGB32" => Some(PPC_Q3_PIXEL_TYPE_ARGB32),
        "RGB16" => Some(PPC_Q3_PIXEL_TYPE_RGB16),
        "ARGB16" => Some(PPC_Q3_PIXEL_TYPE_ARGB16),
        "RGB16_565" => Some(PPC_Q3_PIXEL_TYPE_RGB16_565),
        "RGB24" => Some(PPC_Q3_PIXEL_TYPE_RGB24),
        _ => None,
    }
}

fn endian(value: &str) -> Option<u32> {
    match value {
        "BigEndian" => Some(0),
        "LittleEndian" => Some(1),
        _ => None,
    }
}

fn encode_mipmap(node: &Node<'_>) -> Option<Vec<u8>> {
    if !node.children.is_empty() || node.values.len() < 9 {
        return None;
    }
    let mut body = Vec::new();
    word(&mut body, boolean(node.values[0])?);
    word(&mut body, pixel_type(node.values[1])?);
    word(&mut body, endian(node.values[2])?);
    word(&mut body, endian(node.values[3])?);
    let width = integer(node.values[4])?;
    let height = integer(node.values[5])?;
    let row_bytes = integer(node.values[6])?;
    for value in [width, height, row_bytes, integer(node.values[7])?] {
        word(&mut body, value);
    }
    for value in &node.values[8..] {
        let hex = value.strip_prefix("0x")?;
        if hex.len() % 2 != 0 {
            return None;
        }
        for pair in hex.as_bytes().chunks_exact(2) {
            let pair = std::str::from_utf8(pair).ok()?;
            body.push(u8::from_str_radix(pair, 16).ok()?);
        }
    }
    let image_size = height.checked_mul(row_bytes)?;
    (body.len() == usize::try_from(image_size.checked_add(32)?).ok()?)
        .then(|| body)
        .and_then(|body| chunk(b"txmm", body))
}

fn encode_node(node: &Node<'_>, mesh: Option<[u32; 6]>) -> Option<Vec<u8>> {
    match node.name {
        "Container" => {
            if !node.values.is_empty() {
                return None;
            }
            let local_mesh = node.children.iter().find_map(trimesh_counts).or(mesh);
            let mut body = Vec::new();
            let mut pending_mesh: Option<Vec<u8>> = None;
            for child in &node.children {
                match child.name {
                    "TriMesh" => {
                        if let Some(mesh_body) = pending_mesh.take() {
                            body.extend_from_slice(&chunk(b"tmsh", mesh_body)?);
                        }
                        pending_mesh = Some(encode_trimesh(child)?);
                    }
                    "AttributeArray" if pending_mesh.is_some() => {
                        pending_mesh
                            .as_mut()?
                            .extend_from_slice(&encode_attribute_array(child, local_mesh?)?);
                    }
                    _ => {
                        if let Some(mesh_body) = pending_mesh.take() {
                            body.extend_from_slice(&chunk(b"tmsh", mesh_body)?);
                        }
                        body.extend_from_slice(&encode_node(child, local_mesh)?);
                    }
                }
            }
            if let Some(mesh_body) = pending_mesh {
                body.extend_from_slice(&chunk(b"tmsh", mesh_body)?);
            }
            chunk(b"cntr", body)
        }
        "TriMesh" => chunk(b"tmsh", encode_trimesh(node)?),
        "AttributeArray" => encode_attribute_array(node, mesh?),
        "AttributeSet" if node.values.is_empty() && node.children.is_empty() => {
            chunk(b"attr", Vec::new())
        }
        "TextureShader" if node.values.is_empty() && node.children.is_empty() => {
            chunk(b"txsu", Vec::new())
        }
        "MipmapTexture" => encode_mipmap(node),
        _ => None,
    }
}

/// Convert supported text 3DMF objects into their standard binary 3DMF form.
/// A caller must keep the original source bytes when this returns `None`.
pub(super) fn to_binary(input: &[u8]) -> Option<Vec<u8>> {
    if input.len() > 128 * 1024 * 1024 {
        return None;
    }
    let input = std::str::from_utf8(input).ok()?;
    let nodes = parse(input)?;
    let header = nodes.first()?;
    if header.name != "3DMetafile" || header.values.len() != 4 || !header.children.is_empty() {
        return None;
    }
    let major = u16::try_from(integer(header.values[0])?).ok()?;
    let minor = u16::try_from(integer(header.values[1])?).ok()?;
    if major != 1 || header.values[2] != "Normal" {
        return None;
    }
    let mut header_body = Vec::new();
    header_body.extend_from_slice(&major.to_be_bytes());
    header_body.extend_from_slice(&minor.to_be_bytes());
    word(&mut header_body, 0);
    header_body.extend_from_slice(&0u64.to_be_bytes());
    let mut binary = chunk(b"3DMF", header_body)?;
    for node in &nodes[1..] {
        if node.name != "TableOfContents" {
            binary.extend_from_slice(&encode_node(node, None)?);
        }
    }
    Some(binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_nested_text_mesh_and_texture_to_binary_chunks() {
        let text = b"3DMetafile ( 1 6 Normal toc> )\rmesh: Container ( TriMesh ( 1 0 0 0 3 1 0 1 2 0 0 0 1 0 0 0 1 0 0 0 0 1 1 1 False ) AttributeArray ( 3 0 2 0 0 0 0 1 0 0 1 0 0 1 ) Container ( AttributeSet ( ) Container ( TextureShader ( ) MipmapTexture ( False RGB16 BigEndian BigEndian 2 1 4 0 0x12345678 ) ) ) )";
        let binary = to_binary(text).expect("supported text 3DMF should convert");
        assert!(binary.starts_with(b"3DMF\0\0\0\x10"));
        assert!(binary.windows(4).any(|bytes| bytes == b"tmsh"));
        assert!(binary.windows(4).any(|bytes| bytes == b"atar"));
        assert!(binary.windows(4).any(|bytes| bytes == b"txmm"));
        assert!(binary.ends_with(&[0x12, 0x34, 0x56, 0x78]));
    }

    #[test]
    fn rejects_unrepresented_or_truncated_text_objects() {
        assert!(to_binary(b"3DMetafile ( 1 6 Normal toc> ) Reference ( 1 )").is_none());
        assert!(to_binary(b"3DMetafile ( 1 6 Normal toc> ) Container (").is_none());
    }
}
