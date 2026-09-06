//! Bundled font family selection; assets retain their upstream names and licences.
use super::*;

pub(super) fn bytes(font_id: i16) -> Option<&'static [u8]> {
    Some(match font_id {
        FONT_CHICAGO => include_bytes!("urw/NimbusSans-Bold.ttf"),
        FONT_APPLICATION | FONT_GENEVA | FONT_HELVETICA => {
            include_bytes!("urw/NimbusSans-Regular.ttf")
        }
        FONT_MONACO | FONT_COURIER => include_bytes!("urw/NimbusMonoPS-Regular.ttf"),
        FONT_NEWYORK | FONT_TIMES => include_bytes!("urw/NimbusRoman-Regular.ttf"),
        FONT_PALATINO => include_bytes!("urw/P052-Roman.ttf"),
        FONT_VENICE => include_bytes!("urw/Z003-MediumItalic.ttf"),
        FONT_LONDON => include_bytes!("urw/C059-Bold.ttf"),
        FONT_CAIRO => include_bytes!("urw/URWGothic-Demi.ttf"),
        _ => return None,
    })
}
