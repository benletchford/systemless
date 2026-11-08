//! Shared Color Picker conversions using unsigned 16-bit SmallFract components.

pub(crate) fn hsl_to_rgb(hue: u16, saturation: u16, lightness: u16) -> [u16; 3] {
    fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        } else if t > 1.0 {
            t -= 1.0;
        }

        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }
    let hue = f64::from(hue) / 65535.0;
    let saturation = f64::from(saturation) / 65535.0;
    let lightness = f64::from(lightness) / 65535.0;
    let (red, green, blue) = if saturation == 0.0 {
        (lightness, lightness, lightness)
    } else {
        let q = if lightness < 0.5 {
            lightness * (1.0 + saturation)
        } else {
            lightness + saturation - lightness * saturation
        };
        let p = 2.0 * lightness - q;
        (
            hue_to_rgb(p, q, hue + 1.0 / 3.0),
            hue_to_rgb(p, q, hue),
            hue_to_rgb(p, q, hue - 1.0 / 3.0),
        )
    };
    let to_word = |component: f64| -> u16 { (component.clamp(0.0, 1.0) * 65535.0).round() as u16 };
    [to_word(red), to_word(green), to_word(blue)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl2rgb_small_fract_endpoints_and_primaries() {
        for hue in [0, 0x1357, 0x5555, 0xaaaa, 0xffff] {
            assert_eq!(hsl_to_rgb(hue, 0, 0x2468), [0x2468; 3]);
            assert_eq!(hsl_to_rgb(hue, 0xffff, 0), [0; 3]);
            assert_eq!(hsl_to_rgb(hue, 0xffff, 0xffff), [0xffff; 3]);
        }
        assert_eq!(hsl_to_rgb(0, 0xffff, 0x7fff), [0xfffe, 0, 0]);
        assert_eq!(hsl_to_rgb(0xffff, 0xffff, 0x7fff), [0xfffe, 0, 0]);
        assert_eq!(hsl_to_rgb(0x5555, 0xffff, 0x7fff), [0, 0xfffe, 0]);
        assert_eq!(hsl_to_rgb(0xaaaa, 0xffff, 0x7fff), [0, 0, 0xfffe]);
        assert_eq!(hsl_to_rgb(0, 0xffff, 0x8000), [0xffff, 1, 1]);
    }
}
