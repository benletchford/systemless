use systemless_future::region_geometry::decode;
fn bytes(words: &[i16]) -> Vec<u8> {
    let mut b: Vec<_> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    let n = b.len() as u16;
    b[..2].copy_from_slice(&n.to_be_bytes());
    b
}
#[test]
fn inversion_rows_form_holes_and_disjoint_spans_with_half_open_edges() {
    let donut = bytes(&[
        0, 0, 0, 6, 6, 0, 0, 6, 32767, 2, 2, 4, 32767, 4, 2, 4, 32767, 6, 0, 6, 32767, 32767,
    ]);
    let r = decode(&donut).unwrap();
    for y in -1..8 {
        for x in -1..8 {
            let actual = r
                .iter()
                .filter(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
                .count();
            let expected = (0..6).contains(&x)
                && (0..6).contains(&y)
                && !((2..4).contains(&x) && (2..4).contains(&y));
            assert_eq!(actual, usize::from(expected), "{x},{y}");
        }
    }
    let disjoint = bytes(&[
        0, -2, -6, 1, 0, -2, -6, -4, -2, 0, 32767, 1, -6, -4, -2, 0, 32767, 32767,
    ]);
    let r = decode(&disjoint).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(
        (r[0].left, r[0].top, r[0].right, r[0].bottom),
        (-6, -2, -4, 1)
    );
    assert_eq!((r[1].left, r[1].right), (-2, 0));
}
#[test]
fn malformed_rows_bounds_and_truncation_fail_without_panics() {
    let good = bytes(&[
        0, 0, 0, 4, 4, 0, 0, 4, 32767, 2, 2, 4, 32767, 4, 0, 2, 32767, 32767,
    ]);
    for n in 0..good.len() {
        let mut b = good[..n].to_vec();
        if n >= 2 {
            b[..2].copy_from_slice(&(n as u16).to_be_bytes());
        }
        if n != 10 {
            assert!(decode(&b).is_err(), "truncation {n}");
        }
    }
    for (offset, value) in [(6, 5i16), (8, 5), (18, -1), (14, 0), (32, 0), (34, 0)] {
        let mut bad = good.clone();
        bad[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        assert!(decode(&bad).is_err(), "offset {offset}");
    }
    assert!(decode(&bytes(&[0, 0, 0, 0, 0])).unwrap().is_empty());
}
#[test]
fn expansion_is_bounded_independently_of_encoded_size() {
    let mut words = vec![0, 0, 0, 1200, 2400];
    for y in 0..1200i16 {
        words.extend([y, y * 2, y * 2 + 1, 32767]);
    }
    words.push(1200);
    words.extend(0..2400);
    words.extend([32767, 32767]);
    let b = bytes(&words);
    assert!(b.len() < 32766);
    assert!(decode(&b).is_err());
}
