# Control font freshness checkpoint

Current shared frontend builds successfully (28.75s). Five isolated paint tests
pass, including rejection after font mutation and recovery after genuine redraw.
The actual PPC16 guest applies Geneva 18 bold via Option-F and toggles Checkbox
by clicking its title at scale1.5. Assertions preserve generation, bounds and
font metadata while value changes to1. The shared composed image was inspected:
smooth control labels, original checkbox state and preserved background.
Application-drawn headings remain native bitmap text.

The first command rejected an unsupported explicit --screen-depth16 argument
before guest execution. The successful command uses the supported default PPC16
mode. Its actual capture sidecar, log, guest/composed PNGs and source/executable
hashes are retained here. This selected interaction does not prove physical
host click delivery, all CPU/scale/states, lifecycle or independent font fidelity.
All production gates remain open.

The identical executable also completes mono68k scale0.75, colour68k
scale1.5 and PPC8 scale1 captures. All three assert real guest checkbox toggling
and preserved font/bounds/generation; all composed images were reviewed.
These are selected active-state checks across four modes, not the full scale
or inactive-state matrix or physical host delivery.
