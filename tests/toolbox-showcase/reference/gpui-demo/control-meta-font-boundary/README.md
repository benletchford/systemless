# Control meta-font signed boundary

The PPC Appearance control-title resolver previously negated the guest's signed
16-bit font field in that type. `i16::MIN` overflowed during meta-font decoding.
Decoding now widens to i32 before negation, preserving the existing default for
unknown meta-font IDs. Known IDs, theme IDs and size flags remain unchanged.

The existing `control_title_style_resolves_appearance_meta_fonts` regression
now includes `i16::MIN` and unknown ID -5. The actual library test passes:
1 passed, 0 failed, 6701 filtered out. Terminal output and source hashes are
retained here. This does not qualify GPUI ControlFontStyle overrides or
cross-CPU control appearance; those are explicit inventory gaps.
