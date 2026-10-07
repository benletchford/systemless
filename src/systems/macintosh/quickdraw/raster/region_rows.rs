//! Interval algebra over one region scanline.
//!
//! A row is a flat list of x endpoints: consecutive pairs bound covered
//! intervals, as `endpoints_contain_point` reads them. The functions here never
//! read guest memory; each caller parses its own region storage and keeps its
//! own policy for malformed regions.

#[cfg(test)]
pub(crate) mod tests;

#[derive(Clone, Copy)]
pub(crate) enum RegionBooleanOp {
    Intersection,
    Union,
    Difference,
    Xor,
}

pub(crate) fn clamp_coord(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

/// The non-empty `(start, end)` pairs of `endpoints`, in row order. A trailing
/// odd endpoint is dropped.
pub(crate) fn endpoints_to_intervals(endpoints: &[i16]) -> Vec<(i16, i16)> {
    endpoints
        .chunks_exact(2)
        .filter_map(|pair| (pair[0] < pair[1]).then_some((pair[0], pair[1])))
        .collect()
}

/// Sorts `intervals`, merges overlapping and touching ones, and flattens them
/// back into strictly increasing endpoints.
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

/// Intersection by a single merge pass. Only intervals that the pass meets
/// are compared, so the result is the full intersection only when each row
/// lists its intervals in order of their starts (true of every row produced
/// by [`intervals_to_endpoints`]); unordered rows can lose pieces. The 68K
/// trap path uses this form directly; [`intersect_rows`] checks the order
/// first.
pub(crate) fn intersect_sorted_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let lhs = endpoints_to_intervals(lhs);
    let rhs = endpoints_to_intervals(rhs);
    intervals_to_endpoints(merge_intersect_intervals(&lhs, &rhs))
}

/// Intersection of any two rows. Rows parsed from guest region storage may
/// list their intervals out of order; those are compared all-pairs. Rows
/// whose interval starts never decrease take the merge pass, which gives the
/// same result.
pub(crate) fn intersect_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let lhs = endpoints_to_intervals(lhs);
    let rhs = endpoints_to_intervals(rhs);
    let out = if starts_in_order(&lhs) && starts_in_order(&rhs) {
        merge_intersect_intervals(&lhs, &rhs)
    } else {
        all_pairs_intersect_intervals(&lhs, &rhs)
    };
    intervals_to_endpoints(out)
}

/// Whether the interval starts never decrease, which is all the merge pass
/// needs. The pass moves past an interval once it ends no later than the
/// other row's current interval; every later interval of the other row
/// starts no earlier than that current one, so its overlap with the interval
/// left behind lies inside the overlap the pass already emitted. Touching,
/// overlapping and nested intervals are therefore fine; only a start below
/// an earlier one, as in `[5, 6, 0, 1]`, can drop a piece.
fn starts_in_order(intervals: &[(i16, i16)]) -> bool {
    intervals.windows(2).all(|pair| pair[0].0 <= pair[1].0)
}

fn merge_intersect_intervals(lhs: &[(i16, i16)], rhs: &[(i16, i16)]) -> Vec<(i16, i16)> {
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

    out
}

fn all_pairs_intersect_intervals(lhs: &[(i16, i16)], rhs: &[(i16, i16)]) -> Vec<(i16, i16)> {
    let mut out = Vec::new();
    for &(lhs_start, lhs_end) in lhs {
        for &(rhs_start, rhs_end) in rhs {
            let start = lhs_start.max(rhs_start);
            let end = lhs_end.min(rhs_end);
            if start < end {
                out.push((start, end));
            }
        }
    }
    out
}

pub(crate) fn union_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let mut intervals = endpoints_to_intervals(lhs);
    intervals.extend(endpoints_to_intervals(rhs));
    intervals_to_endpoints(intervals)
}

pub(crate) fn difference_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
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

pub(crate) fn xor_rows(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
    let mut intervals = endpoints_to_intervals(&difference_rows(lhs, rhs));
    intervals.extend(endpoints_to_intervals(&difference_rows(rhs, lhs)));
    intervals_to_endpoints(intervals)
}

/// `Intersection` uses [`intersect_sorted_rows`] and so inherits its ordering
/// precondition.
pub(crate) fn combine_rows(lhs: &[i16], rhs: &[i16], op: RegionBooleanOp) -> Vec<i16> {
    match op {
        RegionBooleanOp::Intersection => intersect_sorted_rows(lhs, rhs),
        RegionBooleanOp::Union => union_rows(lhs, rhs),
        RegionBooleanOp::Difference => difference_rows(lhs, rhs),
        RegionBooleanOp::Xor => xor_rows(lhs, rhs),
    }
}

/// Moves each interval's edges inward by `dh` (outward when negative) and
/// drops intervals that vanish.
pub(crate) fn inset_row(row: &[i16], dh: i16) -> Vec<i16> {
    if row.is_empty() {
        return Vec::new();
    }

    let dh = i32::from(dh);
    let intervals = endpoints_to_intervals(row)
        .into_iter()
        .filter_map(|(left, right)| {
            let new_left = i32::from(left) + dh;
            let new_right = i32::from(right) - dh;
            (new_left < new_right).then_some((clamp_coord(new_left), clamp_coord(new_right)))
        })
        .collect::<Vec<_>>();
    intervals_to_endpoints(intervals)
}

/// Applies one inversion-point list to the active row: a value present in
/// both cancels, every other value is kept, and two sorted inputs give a
/// sorted output.
pub(crate) fn merge_endpoints(lhs: &[i16], rhs: &[i16]) -> Vec<i16> {
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
