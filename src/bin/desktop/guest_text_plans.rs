fn qualify_list_text_fields(
    lists: &[ListManagerSnapshot],
    native: &[u8],
    width: u32,
    height: u32,
) -> Vec<std::collections::BTreeMap<(i16, i16), super::text::ClassicListCellPaintPlan>> {
    lists
        .iter()
        .map(|list| {
            let mut plans = std::collections::BTreeMap::new();
            if list.definition_id != 0 || !list.draw_enabled {
                return plans;
            }
            let Some(global) = list.global_view_rect else {
                return plans;
            };
            for (&cell, paint) in &list.standard_cell_paint {
                let offset_y = i32::from(global.0) - i32::from(list.view_rect.0);
                let offset_x = i32::from(global.1) - i32::from(list.view_rect.1);
                let bounds = [
                    i32::from(paint.clip.0) + offset_y,
                    i32::from(paint.clip.1) + offset_x,
                    i32::from(paint.clip.2) + offset_y,
                    i32::from(paint.clip.3) + offset_x,
                ];
                let Some(bounds) = bounds
                    .into_iter()
                    .map(|value| i16::try_from(value).ok())
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                if let Some(plan) = super::text::ClassicListCellPaintPlan::from_guest(
                    paint,
                    (bounds[0], bounds[1], bounds[2], bounds[3]),
                    native,
                    width,
                    height,
                ) {
                    plans.insert(cell, plan);
                }
            }
            plans
        })
        .collect()
}

// White is an explicit classic-field candidate, not inferred host theme
// paint. Every field pixel must match before the recipe can be presented.
fn qualify_styled_text_fields(
    records: &[TextEditSnapshot],
    native: &[u8],
    width: u32,
    height: u32,
) -> Vec<Option<super::text::StyledTextEditPaintPlan>> {
    records
        .iter()
        .map(|record| {
            let background = systemless::runner::TextEditInkSnapshot {
                pixel: if record.paint.as_ref()?.depth == 16 {
                    0x7fff
                } else {
                    0
                },
                rgb: [255, 255, 255],
                inverted_rgb: [0, 0, 0],
            };
            let caret = (0..record.line_count).find_map(|index| styled_caret_paint(record, index));
            super::text::StyledTextEditPaintPlan::qualify(
                record,
                &background,
                caret.clone(),
                native,
                width,
                height,
            )
            .or_else(|| {
                super::text::StyledTextEditPaintPlan::qualify(
                    record,
                    record.paint.as_ref()?.background_ink.as_ref()?,
                    caret,
                    native,
                    width,
                    height,
                )
            })
        })
        .collect()
}

/// Classic PPC uses the insertion style; 68k uses retained native solid
/// paint evidence, independently of the restored caller foreground.
/// This is a classic-theme candidate; full native pixel qualification
/// rejects a changed theme or unsupported caret paint.
fn styled_caret_paint(
    record: &systemless::runner::TextEditSnapshot,
    index: usize,
) -> Option<(
    (i16, i16, i16, i16),
    systemless::runner::TextEditInkSnapshot,
)> {
    if record.line_layout_policy
        == systemless::runner::TextEditLineLayoutPolicy::CumulativeGuestMetrics
    {
        if !record.active || !record.caret_visible || record.selection.0 != record.selection.1 {
            return None;
        }
        let paint = record.paint.as_ref()?.solid_caret.as_ref()?;
        let (owner, _) = record.caret_line()?;
        let width = paint.0 .3.checked_sub(paint.0 .1)?;
        return (owner == index && record.guest_caret_rect(index, width)?? == paint.0)
            .then(|| paint.clone());
    }
    let rect = record.guest_caret_rect(index, 1)??;
    let (owner, offset) = record.caret_line()?;
    if owner != index {
        return None;
    }
    let insertion = record
        .line_starts
        .as_ref()?
        .get(index)?
        .checked_add(offset)?;
    let run = if record.styled {
        record
            .style_runs
            .as_ref()?
            .iter()
            .rposition(|run| run.start <= insertion)?
    } else {
        0
    };
    Some((rect, record.paint.as_ref()?.style_ink.get(run)?.clone()))
}
