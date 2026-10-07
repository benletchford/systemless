//! The shared row operations must give the same output as the copies they
//! replaced: the `TrapDispatcher` associated functions (68K) and the
//! `ppc_region_*` functions (PowerPC), kept below as references with their logic unchanged.
//! The 68K picture parser held a third copy of `merge_region_endpoints`,
//! identical to the `TrapDispatcher` one.
//! Intersection is the exception: both paths now take the all-pairs pass
//! when a row's interval starts fall, so the 68K merge reference is matched
//! only on rows whose starts are in order.

use super::*;

/// The 68K implementations as they stood before the move, with `Self::`
/// dropped.
pub(crate) mod reference_68k {
    use super::RegionBooleanOp;

    pub(crate) fn endpoints_to_intervals(endpoints: &[i16]) -> Vec<(i16, i16)> {
        endpoints
            .chunks_exact(2)
            .filter_map(|pair| (pair[0] < pair[1]).then_some((pair[0], pair[1])))
            .collect()
    }

    pub(crate) fn intervals_to_endpoints(mut intervals: Vec<(i16, i16)>) -> Vec<i16> {
        if intervals.is_empty() {
            return Vec::new();
        }

        intervals.sort_unstable();
        let mut merged: Vec<(i16, i16)> = Vec::with_capacity(intervals.len());
        for (start, end) in intervals {
            if start >= end {
                continue;
            }
            if let Some((_, last_end)) = merged.last_mut() {
                if start <= *last_end {
                    *last_end = (*last_end).max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }

        let mut endpoints = Vec::with_capacity(merged.len() * 2);
        for (start, end) in merged {
            endpoints.push(start);
            endpoints.push(end);
        }
        endpoints
    }

    pub(crate) fn intersect_region_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let lhs = endpoints_to_intervals(lhs);
        let rhs = endpoints_to_intervals(rhs);
        let mut out = Vec::new();
        let mut lhs_index = 0usize;
        let mut rhs_index = 0usize;

        while let (Some(&(lhs_start, lhs_end)), Some(&(rhs_start, rhs_end))) =
            (lhs.get(lhs_index), rhs.get(rhs_index))
        {
            let start = lhs_start.max(rhs_start);
            let end = lhs_end.min(rhs_end);
            if start < end {
                out.push((start, end));
            }
            if lhs_end < rhs_end {
                lhs_index += 1;
            } else {
                rhs_index += 1;
            }
        }

        intervals_to_endpoints(out)
    }

    pub(crate) fn union_region_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let mut intervals = endpoints_to_intervals(lhs);
        intervals.extend(endpoints_to_intervals(rhs));
        intervals_to_endpoints(intervals)
    }

    pub(crate) fn difference_region_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let lhs = endpoints_to_intervals(lhs);
        let rhs = endpoints_to_intervals(rhs);
        let mut out = Vec::new();

        for (lhs_start, lhs_end) in lhs {
            let mut start = lhs_start;
            for &(rhs_start, rhs_end) in rhs.iter() {
                if rhs_end <= start {
                    continue;
                }
                if rhs_start >= lhs_end {
                    break;
                }
                if rhs_start > start {
                    out.push((start, rhs_start.min(lhs_end)));
                }
                start = start.max(rhs_end);
                if start >= lhs_end {
                    break;
                }
            }
            if start < lhs_end {
                out.push((start, lhs_end));
            }
        }

        intervals_to_endpoints(out)
    }

    pub(crate) fn xor_region_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let mut intervals = endpoints_to_intervals(&difference_region_rows(lhs, rhs));
        intervals.extend(endpoints_to_intervals(&difference_region_rows(rhs, lhs)));
        intervals_to_endpoints(intervals)
    }

    pub(crate) fn combine_region_rows(lhs: &[i16], rhs: &[i16], op: RegionBooleanOp) -> Vec<i16> {
        match op {
            RegionBooleanOp::Intersection => intersect_region_rows(lhs, rhs),
            RegionBooleanOp::Union => union_region_rows(lhs, rhs),
            RegionBooleanOp::Difference => difference_region_rows(lhs, rhs),
            RegionBooleanOp::Xor => xor_region_rows(lhs, rhs),
        }
    }

    pub(crate) fn clamp_region_coord(value: i32) -> i16 {
        value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }

    pub(crate) fn inset_region_row(row: &[i16], dh: i16) -> Vec<i16> {
        if row.is_empty() {
            return Vec::new();
        }

        let dh = i32::from(dh);
        let intervals = endpoints_to_intervals(row)
            .into_iter()
            .filter_map(|(left, right)| {
                let new_left = i32::from(left) + dh;
                let new_right = i32::from(right) - dh;
                (new_left < new_right)
                    .then_some((clamp_region_coord(new_left), clamp_region_coord(new_right)))
            })
            .collect::<Vec<_>>();
        intervals_to_endpoints(intervals)
    }

    pub(crate) fn merge_region_endpoints(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let mut merged = Vec::with_capacity(lhs.len() + rhs.len());
        let mut lhs_index = 0usize;
        let mut rhs_index = 0usize;

        while lhs_index < lhs.len() || rhs_index < rhs.len() {
            match (lhs.get(lhs_index), rhs.get(rhs_index)) {
                (Some(&lhs_value), Some(&rhs_value)) if lhs_value < rhs_value => {
                    merged.push(lhs_value);
                    lhs_index += 1;
                }
                (Some(&lhs_value), Some(&rhs_value)) if rhs_value < lhs_value => {
                    merged.push(rhs_value);
                    rhs_index += 1;
                }
                (Some(_), Some(_)) => {
                    lhs_index += 1;
                    rhs_index += 1;
                }
                (Some(&lhs_value), None) => {
                    merged.push(lhs_value);
                    lhs_index += 1;
                }
                (None, Some(&rhs_value)) => {
                    merged.push(rhs_value);
                    rhs_index += 1;
                }
                (None, None) => break,
            }
        }

        merged
    }

    pub(crate) fn endpoints_contain_point(endpoints: &[i16], x: i16) -> bool {
        let mut in_region = false;
        for &edge in endpoints {
            if edge > x {
                break;
            }
            in_region = !in_region;
        }
        in_region
    }
}

/// The PowerPC implementations as they stood before the move.
pub(crate) mod reference_ppc {
    #[derive(Clone, Copy)]
    pub(crate) enum PpcRegionBooleanOp {
        Intersection,
        Union,
        Difference,
        Xor,
    }

    pub(crate) fn ppc_region_merge_endpoints(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let mut merged = Vec::with_capacity(lhs.len() + rhs.len());
        let mut lhs_index = 0usize;
        let mut rhs_index = 0usize;
        while lhs_index < lhs.len() || rhs_index < rhs.len() {
            match (lhs.get(lhs_index), rhs.get(rhs_index)) {
                (Some(&lhs_value), Some(&rhs_value)) if lhs_value < rhs_value => {
                    merged.push(lhs_value);
                    lhs_index += 1;
                }
                (Some(&lhs_value), Some(&rhs_value)) if rhs_value < lhs_value => {
                    merged.push(rhs_value);
                    rhs_index += 1;
                }
                (Some(_), Some(_)) => {
                    lhs_index += 1;
                    rhs_index += 1;
                }
                (Some(&lhs_value), None) => {
                    merged.push(lhs_value);
                    lhs_index += 1;
                }
                (None, Some(&rhs_value)) => {
                    merged.push(rhs_value);
                    rhs_index += 1;
                }
                (None, None) => break,
            }
        }
        merged
    }

    pub(crate) fn ppc_region_endpoints_to_intervals(endpoints: &[i16]) -> Vec<(i16, i16)> {
        endpoints
            .chunks_exact(2)
            .filter_map(|pair| (pair[0] < pair[1]).then_some((pair[0], pair[1])))
            .collect()
    }

    pub(crate) fn ppc_region_intervals_to_endpoints(mut intervals: Vec<(i16, i16)>) -> Vec<i16> {
        intervals.sort_unstable();
        let mut merged: Vec<(i16, i16)> = Vec::with_capacity(intervals.len());
        for (start, end) in intervals {
            if start >= end {
                continue;
            }
            if let Some((_, last_end)) = merged.last_mut() {
                if start <= *last_end {
                    *last_end = (*last_end).max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }
        merged
            .into_iter()
            .flat_map(|(start, end)| [start, end])
            .collect()
    }

    pub(crate) fn ppc_region_intersect_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let lhs = ppc_region_endpoints_to_intervals(lhs);
        let rhs = ppc_region_endpoints_to_intervals(rhs);
        let mut out = Vec::new();
        for &(lhs_start, lhs_end) in &lhs {
            for &(rhs_start, rhs_end) in &rhs {
                let start = lhs_start.max(rhs_start);
                let end = lhs_end.min(rhs_end);
                if start < end {
                    out.push((start, end));
                }
            }
        }
        ppc_region_intervals_to_endpoints(out)
    }

    pub(crate) fn ppc_region_union_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let mut intervals = ppc_region_endpoints_to_intervals(lhs);
        intervals.extend(ppc_region_endpoints_to_intervals(rhs));
        ppc_region_intervals_to_endpoints(intervals)
    }

    pub(crate) fn ppc_region_difference_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
        let lhs = ppc_region_endpoints_to_intervals(lhs);
        let rhs = ppc_region_endpoints_to_intervals(rhs);
        let mut out = Vec::new();
        for (lhs_start, lhs_end) in lhs {
            let mut start = lhs_start;
            for &(rhs_start, rhs_end) in &rhs {
                if rhs_end <= start {
                    continue;
                }
                if rhs_start >= lhs_end {
                    break;
                }
                if rhs_start > start {
                    out.push((start, rhs_start.min(lhs_end)));
                }
                start = start.max(rhs_end);
                if start >= lhs_end {
                    break;
                }
            }
            if start < lhs_end {
                out.push((start, lhs_end));
            }
        }
        ppc_region_intervals_to_endpoints(out)
    }

    pub(crate) fn ppc_region_combine_rows(
        lhs: &[i16],
        rhs: &[i16],
        operation: PpcRegionBooleanOp,
    ) -> Vec<i16> {
        match operation {
            PpcRegionBooleanOp::Intersection => ppc_region_intersect_rows(lhs, rhs),
            PpcRegionBooleanOp::Union => ppc_region_union_rows(lhs, rhs),
            PpcRegionBooleanOp::Difference => ppc_region_difference_rows(lhs, rhs),
            PpcRegionBooleanOp::Xor => ppc_region_union_rows(
                &ppc_region_difference_rows(lhs, rhs),
                &ppc_region_difference_rows(rhs, lhs),
            ),
        }
    }
}

/// Deterministic xorshift generator, so failures reproduce.
pub(crate) struct RowRng(u64);

impl RowRng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A coordinate from a narrow window, so equal, touching and nested
    /// edges are common; occasionally an extreme value.
    pub(crate) fn coord(&mut self) -> i16 {
        match self.below(32) {
            0 => i16::MIN,
            1 => i16::MAX,
            2 => i16::MAX - 1,
            3 => i16::MIN + 1,
            _ => self.below(28) as i16 - 6,
        }
    }

    /// Any endpoint list, including odd lengths, unordered values, empty and
    /// inverted pairs.
    pub(crate) fn arbitrary_row(&mut self) -> Vec<i16> {
        let len = self.below(10) as usize;
        (0..len).map(|_| self.coord()).collect()
    }

    /// Endpoints that never decrease. Even-length rows with strictly
    /// increasing values are what a well-formed region parses to; repeated
    /// values add touching intervals and empty pairs.
    pub(crate) fn sorted_row(&mut self) -> Vec<i16> {
        let mut row = self.arbitrary_row();
        if self.below(2) == 0 {
            row.truncate(row.len() & !1);
        }
        row.sort_unstable();
        if self.below(2) == 0 {
            row.dedup();
            row.truncate(row.len() & !1);
        }
        row
    }

    pub(crate) fn intervals(&mut self) -> Vec<(i16, i16)> {
        let len = self.below(8) as usize;
        (0..len).map(|_| (self.coord(), self.coord())).collect()
    }
}

/// Hand-picked well-formed rows: empty, single point, touching, nested,
/// identical and extreme intervals.
pub(crate) fn edge_rows() -> Vec<Vec<i16>> {
    vec![
        vec![],
        vec![4, 5],
        vec![0, 10],
        vec![2, 3],
        vec![0, 5, 5, 10],
        vec![0, 4, 6, 10],
        vec![-3, 0, 10, 12],
        vec![1, 2, 3, 4, 5, 6],
        vec![3, 3],
        vec![i16::MIN, i16::MAX],
        vec![i16::MAX - 1, i16::MAX],
        vec![i16::MIN, i16::MIN + 1, 0, 1],
    ]
}

const RANDOM_CASES: usize = 50_000;

fn all_ops() -> [RegionBooleanOp; 4] {
    [
        RegionBooleanOp::Intersection,
        RegionBooleanOp::Union,
        RegionBooleanOp::Difference,
        RegionBooleanOp::Xor,
    ]
}

/// Runs `check` over every edge-row pair and `RANDOM_CASES` random pairs.
fn for_row_pairs(seed: u64, sorted: bool, mut check: impl FnMut(&[i16], &[i16])) {
    let edges = edge_rows();
    for lhs in &edges {
        for rhs in &edges {
            check(lhs, rhs);
        }
    }
    let mut rng = RowRng::new(seed);
    for _ in 0..RANDOM_CASES {
        let (lhs, rhs) = if sorted {
            (rng.sorted_row(), rng.sorted_row())
        } else {
            (rng.arbitrary_row(), rng.arbitrary_row())
        };
        check(&lhs, &rhs);
    }
}

#[test]
fn row_operations_match_68k_reference_on_any_rows() {
    for_row_pairs(0x68_0001, false, |lhs, rhs| {
        assert_eq!(
            endpoints_to_intervals(lhs),
            reference_68k::endpoints_to_intervals(lhs),
            "{lhs:?}"
        );
        // The 68K path now takes the all-pairs intersection where a row's
        // interval starts fall; it agreed with the merge pass everywhere else.
        let ordered = starts_in_order(&endpoints_to_intervals(lhs))
            && starts_in_order(&endpoints_to_intervals(rhs));
        let previous_intersection = if ordered {
            reference_68k::intersect_region_rows(lhs, rhs)
        } else {
            reference_ppc::ppc_region_intersect_rows(lhs, rhs)
        };
        assert_eq!(
            intersect_rows(lhs, rhs),
            previous_intersection,
            "intersect {lhs:?} {rhs:?}"
        );
        assert_eq!(
            union_rows(lhs, rhs),
            reference_68k::union_region_rows(lhs, rhs),
            "union {lhs:?} {rhs:?}"
        );
        assert_eq!(
            difference_rows(lhs, rhs),
            reference_68k::difference_region_rows(lhs, rhs),
            "difference {lhs:?} {rhs:?}"
        );
        assert_eq!(
            xor_rows(lhs, rhs),
            reference_68k::xor_region_rows(lhs, rhs),
            "xor {lhs:?} {rhs:?}"
        );
        for op in all_ops() {
            let expected = match op {
                RegionBooleanOp::Intersection => previous_intersection.clone(),
                _ => reference_68k::combine_region_rows(lhs, rhs, op),
            };
            assert_eq!(
                combine_rows(lhs, rhs, op),
                expected,
                "combine {lhs:?} {rhs:?}"
            );
        }
        assert_eq!(
            merge_endpoints(lhs, rhs),
            reference_68k::merge_region_endpoints(lhs, rhs),
            "merge {lhs:?} {rhs:?}"
        );
        for x in -8..=24 {
            assert_eq!(
                endpoints_contain_point(lhs, x),
                reference_68k::endpoints_contain_point(lhs, x),
                "contain {lhs:?} {x}"
            );
        }
        for x in [i16::MIN, i16::MIN + 1, i16::MAX - 1, i16::MAX] {
            assert_eq!(
                endpoints_contain_point(lhs, x),
                reference_68k::endpoints_contain_point(lhs, x),
                "contain {lhs:?} {x}"
            );
        }
    });
}

#[test]
fn inset_and_clamp_match_68k_reference() {
    let insets = [i16::MIN, -300, -3, -1, 0, 1, 2, 5, 300, i16::MAX];
    let check = |row: &[i16]| {
        for dh in insets {
            assert_eq!(
                inset_row(row, dh),
                reference_68k::inset_region_row(row, dh),
                "{row:?} {dh}"
            );
        }
    };
    for row in edge_rows() {
        check(&row);
    }
    let mut rng = RowRng::new(0x68_0002);
    for _ in 0..RANDOM_CASES {
        check(&rng.arbitrary_row());
    }
    for value in [
        i32::MIN,
        -40_000,
        -32_769,
        -32_768,
        -1,
        0,
        32_767,
        32_768,
        i32::MAX,
    ] {
        assert_eq!(clamp_coord(value), reference_68k::clamp_region_coord(value));
    }
}

#[test]
fn intervals_to_endpoints_matches_both_references() {
    let mut rng = RowRng::new(0x68_0003);
    let mut cases = vec![
        vec![],
        vec![(3, 3)],
        vec![(5, 2)],
        vec![(0, 4), (4, 8)],
        vec![(0, 9), (2, 3)],
    ];
    cases.extend((0..RANDOM_CASES).map(|_| rng.intervals()));
    for intervals in cases {
        let shared = intervals_to_endpoints(intervals.clone());
        assert_eq!(
            shared,
            reference_68k::intervals_to_endpoints(intervals.clone()),
            "{intervals:?}"
        );
        assert_eq!(
            shared,
            reference_ppc::ppc_region_intervals_to_endpoints(intervals.clone()),
            "{intervals:?}"
        );
    }
}

/// The operations the PowerPC path now takes from this module.
#[test]
fn shared_operations_match_ppc_reference_on_any_rows() {
    use reference_ppc::PpcRegionBooleanOp;
    for_row_pairs(0x0bc_0001, false, |lhs, rhs| {
        assert_eq!(
            endpoints_to_intervals(lhs),
            reference_ppc::ppc_region_endpoints_to_intervals(lhs),
            "{lhs:?}"
        );
        assert_eq!(
            merge_endpoints(lhs, rhs),
            reference_ppc::ppc_region_merge_endpoints(lhs, rhs),
            "merge {lhs:?} {rhs:?}"
        );
        assert_eq!(
            union_rows(lhs, rhs),
            reference_ppc::ppc_region_union_rows(lhs, rhs),
            "union {lhs:?} {rhs:?}"
        );
        assert_eq!(
            difference_rows(lhs, rhs),
            reference_ppc::ppc_region_difference_rows(lhs, rhs),
            "difference {lhs:?} {rhs:?}"
        );
        assert_eq!(
            xor_rows(lhs, rhs),
            reference_ppc::ppc_region_combine_rows(lhs, rhs, PpcRegionBooleanOp::Xor),
            "xor {lhs:?} {rhs:?}"
        );
    });
}

/// On rows whose endpoints never decrease, the 68K merge-based intersection
/// equals the PowerPC all-pairs one.
#[test]
fn merge_intersection_matches_ppc_all_pairs_on_sorted_rows() {
    for_row_pairs(0x0bc_0002, true, |lhs, rhs| {
        assert_eq!(
            reference_68k::intersect_region_rows(lhs, rhs),
            reference_ppc::ppc_region_intersect_rows(lhs, rhs),
            "{lhs:?} {rhs:?}"
        );
    });
}

/// Unsorted rows are where the two intersections part, which is why
/// `intersect_rows` checks the ordering before taking the merge pass.
#[test]
fn merge_intersection_differs_from_all_pairs_on_unsorted_rows() {
    let lhs = [5, 6, 0, 1];
    let rhs = [0, 1, 5, 6];
    assert_eq!(
        reference_ppc::ppc_region_intersect_rows(&lhs, &rhs),
        vec![0, 1, 5, 6]
    );
    assert_eq!(reference_68k::intersect_region_rows(&lhs, &rhs), vec![5, 6]);
    assert_eq!(intersect_rows(&lhs, &rhs), vec![0, 1, 5, 6]);
    assert_eq!(
        combine_rows(&lhs, &rhs, RegionBooleanOp::Intersection),
        vec![0, 1, 5, 6]
    );
}

/// A row whose non-empty interval starts never decrease, with empty and
/// inverted pairs mixed in. Those pairs are dropped before either
/// intersection runs. Unlike `sorted_row`, intervals may overlap or nest.
fn start_ordered_row(rng: &mut RowRng) -> Vec<i16> {
    let mut intervals = rng.intervals();
    for interval in &mut intervals {
        if interval.0 > interval.1 {
            *interval = (interval.1, interval.0);
        }
    }
    intervals.sort_unstable_by_key(|interval| interval.0);
    let mut row = intervals
        .into_iter()
        .flat_map(|(start, end)| [start, end])
        .collect::<Vec<_>>();
    for _ in 0..rng.next() % 3 {
        let at = (rng.next() as usize % (row.len() / 2 + 1)) * 2;
        let (a, b) = (rng.coord(), rng.coord());
        row.splice(at..at, [a.max(b), a.min(b)]);
    }
    if rng.next() % 4 == 0 {
        row.push(rng.coord());
    }
    row
}

#[test]
fn intersect_rows_matches_ppc_all_pairs_on_any_rows() {
    let check = |lhs: &[i16], rhs: &[i16]| {
        assert_eq!(
            intersect_rows(lhs, rhs),
            reference_ppc::ppc_region_intersect_rows(lhs, rhs),
            "{lhs:?} {rhs:?}"
        );
    };
    for_row_pairs(0x0bc_0004, false, check);
    for_row_pairs(0x0bc_0005, true, check);
    let mut rng = RowRng::new(0x0bc_0006);
    for _ in 0..RANDOM_CASES {
        let lhs = start_ordered_row(&mut rng);
        let rhs = start_ordered_row(&mut rng);
        check(&lhs, &rhs);
    }
}

/// Where the merge pass runs, it is the previous 68K code, and that code
/// agrees with the previous PowerPC all-pairs code.
#[test]
fn intersect_rows_matches_68k_merge_on_start_ordered_rows() {
    let check = |lhs: &[i16], rhs: &[i16]| {
        assert!(starts_in_order(&endpoints_to_intervals(lhs)), "{lhs:?}");
        assert!(starts_in_order(&endpoints_to_intervals(rhs)), "{rhs:?}");
        let merged = reference_68k::intersect_region_rows(lhs, rhs);
        assert_eq!(intersect_rows(lhs, rhs), merged, "{lhs:?} {rhs:?}");
        assert_eq!(
            reference_ppc::ppc_region_intersect_rows(lhs, rhs),
            merged,
            "{lhs:?} {rhs:?}"
        );
    };
    for_row_pairs(0x68_0004, true, check);
    let mut rng = RowRng::new(0x68_0005);
    for _ in 0..RANDOM_CASES {
        let lhs = start_ordered_row(&mut rng);
        let rhs = start_ordered_row(&mut rng);
        check(&lhs, &rhs);
    }
}

#[test]
fn starts_in_order_allows_overlap_but_not_a_falling_start() {
    let cases: [(&[i16], bool); 12] = [
        (&[], true),
        (&[0, 5], true),
        (&[0, 5, 5, 10], true),
        (&[0, 4, 6, 10], true),
        (&[0, 5, 4, 10], true),
        (&[0, 10, 2, 3], true),
        (&[0, 5, 0, 5], true),
        (&[0, 5, 9, 2, 6, 10], true),
        (&[0, 5, 6, 10, 1], true),
        (&[5, 6, 0, 1], false),
        (&[0, 10, 4, 6, 2, 3], false),
        (&[0, 5, 4, 10, 3, 6], false),
    ];
    for (row, expected) in cases {
        assert_eq!(
            starts_in_order(&endpoints_to_intervals(row)),
            expected,
            "{row:?}"
        );
    }
}

#[test]
fn intersect_rows_hand_cases() {
    let cases: [(&[i16], &[i16], &[i16]); 12] = [
        (&[5, 6, 0, 1], &[0, 1, 5, 6], &[0, 1, 5, 6]),
        (&[0, 1, 5, 6], &[5, 6, 0, 1], &[0, 1, 5, 6]),
        (&[0, 5, 5, 10], &[3, 7], &[3, 7]),
        (&[0, 4, 6, 10], &[4, 6], &[]),
        (&[0, 10, 2, 3], &[1, 2, 9, 12], &[1, 2, 9, 10]),
        (&[0, 5, 4, 10], &[3, 6], &[3, 6]),
        (&[0, 10, 7], &[5, 20], &[5, 10]),
        (&[3, 3, 0, 10], &[2, 4], &[2, 4]),
        (&[8, 12, 0, 4], &[2, 10], &[2, 4, 8, 10]),
        (&[0, 20, 2, 4], &[3, 5, 1, 2], &[1, 2, 3, 5]),
        (&[6, 9, 0, 3], &[2, 7], &[2, 3, 6, 7]),
        (
            &[i16::MIN, i16::MAX],
            &[i16::MAX - 1, i16::MAX, 0, 1],
            &[0, 1, i16::MAX - 1, i16::MAX],
        ),
    ];
    for (lhs, rhs, expected) in cases {
        assert_eq!(intersect_rows(lhs, rhs), expected, "{lhs:?} {rhs:?}");
        assert_eq!(
            reference_ppc::ppc_region_intersect_rows(lhs, rhs),
            expected,
            "{lhs:?} {rhs:?}"
        );
    }
}
