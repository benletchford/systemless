//! Translation between painted host glyph boundaries and guest insertion points.

#[derive(Clone, Debug)]
pub(crate) struct TextPointerMap {
    pub identity: (u32, u64),
    pub text: String,
    pub guest_bounds: (i16, i16, i16, i16),
    pub positions: Vec<(f32, i16)>,
}

impl TextPointerMap {
    pub fn horizontal(&self, host_x: f32) -> Option<i16> {
        for pair in self.positions.windows(2) {
            if host_x < (pair[0].0 + pair[1].0) / 2. {
                return Some(pair[0].1);
            }
        }
        self.positions.last().map(|position| position.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_glyph_midpoints_map_to_guest_positions_at_every_scale() {
        for scale in [0.75, 1., 1.5, 2.] {
            let map = TextPointerMap {
                identity: (1, 2), text: "iéW".into(), guest_bounds: (10, 20, 30, 100),
                positions: [0., 3., 12., 25.].into_iter().zip([21, 26, 35, 47])
                    .map(|(x, guest)| (100. + x * scale, guest)).collect(),
            };
            for (x, expected) in [(-20., 21), (1., 21), (2., 26), (7., 26), (8., 35), (18., 35), (19., 47), (50., 47)] {
                assert_eq!(map.horizontal(100. + x * scale), Some(expected));
            }
        }
    }
}
