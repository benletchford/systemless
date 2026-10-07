//! The PowerPC region-row entry points, now partly backed by
//! `quickdraw::raster::region_rows`, must match their previous
//! implementations on any rows.

use super::*;
use crate::quickdraw::raster::region_rows::tests::{edge_rows, reference_ppc, RowRng};

#[test]
fn ppc_region_row_operations_match_previous_implementations() {
    let ops = [
        (
            PpcRegionBooleanOp::Intersection,
            reference_ppc::PpcRegionBooleanOp::Intersection,
        ),
        (
            PpcRegionBooleanOp::Union,
            reference_ppc::PpcRegionBooleanOp::Union,
        ),
        (
            PpcRegionBooleanOp::Difference,
            reference_ppc::PpcRegionBooleanOp::Difference,
        ),
        (
            PpcRegionBooleanOp::Xor,
            reference_ppc::PpcRegionBooleanOp::Xor,
        ),
    ];
    let check = |lhs: &[i16], rhs: &[i16]| {
        assert_eq!(
            ppc_region_intersect_rows(lhs, rhs),
            reference_ppc::ppc_region_intersect_rows(lhs, rhs),
            "intersect {lhs:?} {rhs:?}"
        );
        assert_eq!(
            ppc_region_merge_endpoints(lhs, rhs),
            reference_ppc::ppc_region_merge_endpoints(lhs, rhs),
            "merge {lhs:?} {rhs:?}"
        );
        for (op, reference_op) in ops {
            assert_eq!(
                ppc_region_combine_rows(lhs, rhs, op),
                reference_ppc::ppc_region_combine_rows(lhs, rhs, reference_op),
                "combine {lhs:?} {rhs:?}"
            );
        }
    };
    let edges = edge_rows();
    for lhs in &edges {
        for rhs in &edges {
            check(lhs, rhs);
        }
    }
    let mut rng = RowRng::new(0x0bc_0003);
    for _ in 0..50_000 {
        let (lhs, rhs) = (rng.arbitrary_row(), rng.arbitrary_row());
        check(&lhs, &rhs);
    }
}
