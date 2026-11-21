//! Line metrics that `GetFontInfo` and `FontMetrics` report for the built-in
//! families while their glyphs come from the bundled substitutes.
//!
//! Every value here was measured by calling `GetFontInfo` on Mac OS 9.0 with
//! bitmaps preferred (the default), plain and bold, at every size from 6 to 48
//! points (see `compatibility/README.md`). Each family follows one rule per
//! field at every measured size except a few, which are listed as exceptions.
//! Sizes outside 6..=48 follow the rule. This is metadata only: nothing here
//! comes from a font file.

use super::families::{
    FONT_APPLICATION, FONT_CHICAGO, FONT_COURIER, FONT_GENEVA, FONT_HELVETICA, FONT_MONACO, FONT_NEWYORK,
    FONT_PALATINO, FONT_SYMBOL, FONT_TIMES,
};
use super::FontMetrics;

#[derive(Clone, Copy)]
enum Round {
    /// Halves round up: 2.5 → 3.
    HalfUp,
    /// Halves round down: 2.5 → 2.
    HalfDown,
    Ceil,
}

#[derive(Clone, Copy)]
enum Rule {
    Size,
    Zero,
    /// The point size less the ascent.
    SizeMinusAscent,
    /// `numerator / denominator` of the point size, rounded.
    Scaled(i32, i32, Round),
}

#[derive(Clone, Copy)]
enum BoldWidMax {
    Plain,
    PlainPlusOne,
    Size,
}

/// A size whose measured values break the family's rules: plain ascent,
/// descent, widMax and leading, then bold widMax. Bold ascent, descent and
/// leading equal plain at every measured size.
type Exception = (i16, [i16; 4], i16);

struct Family {
    font_id: i16,
    ascent: Rule,
    descent: Rule,
    wid_max: Rule,
    leading: Rule,
    bold_wid_max: BoldWidMax,
    exceptions: &'static [Exception],
}

use Round::{Ceil, HalfDown, HalfUp};
use Rule::{Scaled, Size, SizeMinusAscent, Zero};

// The ratios are the smallest-denominator fractions that reproduce every
// measured size outside the exceptions.
const FAMILIES: [Family; 9] = [
    Family {
        font_id: FONT_CHICAGO,
        ascent: Size,
        descent: Scaled(1, 4, HalfDown),
        wid_max: Scaled(31, 24, Ceil),
        leading: Scaled(1, 12, HalfUp),
        bold_wid_max: BoldWidMax::Plain,
        exceptions: &[],
    },
    Family {
        font_id: FONT_NEWYORK,
        ascent: Size,
        descent: Scaled(1, 4, HalfDown),
        wid_max: Scaled(41, 32, Ceil),
        leading: Scaled(1, 12, HalfUp),
        bold_wid_max: BoldWidMax::PlainPlusOne,
        exceptions: &[(9, [10, 2, 12, 0], 13), (10, [10, 2, 13, 0], 14)],
    },
    Family {
        font_id: FONT_GENEVA,
        ascent: Size,
        descent: Scaled(1, 4, HalfDown),
        wid_max: Scaled(59, 45, Ceil),
        leading: Scaled(1, 12, HalfUp),
        bold_wid_max: BoldWidMax::PlainPlusOne,
        exceptions: &[(9, [10, 2, 12, 0], 13), (10, [10, 2, 11, 1], 12)],
    },
    Family {
        font_id: FONT_MONACO,
        ascent: Size,
        descent: Scaled(1, 4, HalfDown),
        wid_max: Scaled(53, 44, Ceil),
        leading: Scaled(1, 12, HalfUp),
        bold_wid_max: BoldWidMax::PlainPlusOne,
        exceptions: &[(9, [9, 2, 6, 0], 7)],
    },
    Family {
        font_id: FONT_PALATINO,
        ascent: Scaled(51, 62, HalfUp),
        descent: Scaled(5, 18, HalfDown),
        wid_max: Scaled(55, 48, Ceil),
        leading: Zero,
        bold_wid_max: BoldWidMax::Size,
        exceptions: &[(10, [9, 3, 13, 0], 13)],
    },
    Family {
        font_id: FONT_TIMES,
        ascent: Scaled(3, 4, HalfUp),
        descent: SizeMinusAscent,
        wid_max: Size,
        leading: Zero,
        bold_wid_max: BoldWidMax::Plain,
        exceptions: &[(9, [8, 2, 9, 1], 10), (10, [9, 2, 10, 1], 11)],
    },
    Family {
        font_id: FONT_HELVETICA,
        ascent: Scaled(10, 13, HalfUp),
        descent: SizeMinusAscent,
        wid_max: Scaled(49, 48, Ceil),
        leading: Zero,
        bold_wid_max: BoldWidMax::Size,
        exceptions: &[(9, [8, 2, 9, 1], 10), (10, [9, 2, 10, 1], 11)],
    },
    Family {
        font_id: FONT_COURIER,
        ascent: Scaled(3, 4, HalfUp),
        descent: SizeMinusAscent,
        wid_max: Scaled(14, 17, Ceil),
        leading: Zero,
        bold_wid_max: BoldWidMax::Plain,
        exceptions: &[(9, [8, 2, 8, 1], 9), (10, [9, 2, 9, 1], 10)],
    },
    Family {
        font_id: FONT_SYMBOL,
        ascent: Scaled(7, 10, HalfUp),
        descent: SizeMinusAscent,
        wid_max: Scaled(49, 47, Ceil),
        leading: Zero,
        bold_wid_max: BoldWidMax::Plain,
        exceptions: &[(9, [10, 3, 9, 0], 9), (10, [9, 2, 11, 0], 11)],
    },
];

fn apply(rule: Rule, size: i32, ascent: i32) -> i32 {
    match rule {
        Size => size,
        Zero => 0,
        SizeMinusAscent => size - ascent,
        Scaled(numerator, denominator, round) => {
            let scaled = size * numerator;
            match round {
                HalfUp => (2 * scaled + denominator).div_euclid(2 * denominator),
                HalfDown => (2 * scaled + denominator - 1).div_euclid(2 * denominator),
                Ceil => (scaled + denominator - 1).div_euclid(denominator),
            }
        }
    }
}

/// The measured Mac OS 9.0 line metrics for a built-in family at `size`
/// (0 means 12) in `style`, or `None` for any other family. The application
/// font (1) is Geneva, as Systemless draws it. Only the bold bit of `style`
/// changes them (widMax).
pub(super) fn classic_line_metrics(font_id: i16, size: i16, style: u8) -> Option<FontMetrics> {
    let font_id = if font_id == FONT_APPLICATION {
        FONT_GENEVA
    } else {
        font_id
    };
    let family = FAMILIES.iter().find(|family| family.font_id == font_id)?;
    let size = if size == 0 { 12 } else { size.max(1) };
    let bold = style & 0x01 != 0;
    if let Some(&(_, [ascent, descent, wid_max, leading], bold_wid_max)) = family
        .exceptions
        .iter()
        .find(|exception| exception.0 == size)
    {
        return Some(FontMetrics {
            ascent,
            descent,
            wid_max: if bold { bold_wid_max } else { wid_max },
            leading,
        });
    }
    let size = i32::from(size);
    let ascent = apply(family.ascent, size, 0);
    let wid_max = apply(family.wid_max, size, ascent);
    let wid_max = match (bold, family.bold_wid_max) {
        (false, _) | (true, BoldWidMax::Plain) => wid_max,
        (true, BoldWidMax::PlainPlusOne) => wid_max + 1,
        (true, BoldWidMax::Size) => size,
    };
    let clamp = |value: i32| value.clamp(0, i32::from(i16::MAX)) as i16;
    Some(FontMetrics {
        ascent: clamp(ascent),
        descent: clamp(apply(family.descent, size, ascent)),
        wid_max: clamp(wid_max),
        leading: clamp(apply(family.leading, size, ascent)),
    })
}
