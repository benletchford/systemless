#!/bin/sh
# Uses the repository's pinned MPW image; see tests/toolbox-showcase/Dockerfile.
set -eu
fixture_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output_dir="${fixture_dir}/../../target/mpw-probe"
mkdir -p "${output_dir}"
output_dir=$(CDPATH= cd -- "${output_dir}" && pwd)
cp "${fixture_dir}/timers.c" "${fixture_dir}/sound_info.c" "${fixture_dir}/sound_info.r" "${fixture_dir}/resource_sound.c" "${fixture_dir}/resource_sound.r" "${fixture_dir}/queued_sound.c" "${fixture_dir}/handle_state.c" "${fixture_dir}/handle_state.r" "${fixture_dir}/sampled_sound.c" "${fixture_dir}/color_windows.c" "${fixture_dir}/color_windows.r" "${fixture_dir}/color_ports.c" "${fixture_dir}/devices.c" "${fixture_dir}/sound_channels.c" "${fixture_dir}/persistence.c" "${fixture_dir}/open_file.c" "${fixture_dir}/standard_file.c" "${fixture_dir}/working.c" "${fixture_dir}/menu_tracking.c" "${fixture_dir}/tracking.c" "${fixture_dir}/environment.c" "${fixture_dir}/dialogs.c" "${fixture_dir}/menus.c" "${fixture_dir}/menus.r" "${fixture_dir}/textedit.c" "${fixture_dir}/windows.c" "${fixture_dir}/windows.r" "${fixture_dir}/plugin.c" "${fixture_dir}/dynamic.c" "${fixture_dir}/dynamic.r" "${fixture_dir}/probe.c" "${fixture_dir}/jump.c" "${fixture_dir}/events.c" "${fixture_dir}/wait.c" "${fixture_dir}/draw.c" "${fixture_dir}/probe.r" "${fixture_dir}/build.mpw" "${output_dir}/"
if ! docker run --rm --volume "${output_dir}:/workspace" systemless-toolbox-showcase-mpw %%% /workspace/build.mpw > "${output_dir}/build.log" 2>&1; then
    cat "${output_dir}/build.log"
    exit 1
fi
cat "${output_dir}/build.log"
# MPW ToolServer can report compiler/linker failure while its host exits zero.
if grep -Eq '(^#+.*[Ee]rror|Execution.*terminated)' "${output_dir}/build.log"; then
    exit 1
fi
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw probe.68k.rdump -o probe.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw probe.ppc.rdump -o probe.ppc.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw events.68k.rdump -o events.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw wait.68k.rdump -o wait.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw draw.68k.rdump -o draw.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw draw.ppc.rdump -o draw.ppc.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw dynamic.ppc.rdump -o dynamic.ppc.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw windows.68k.rdump -o windows.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw textedit.68k.rdump -o textedit.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw menus.68k.rdump -o menus.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw menus.ppc.rdump -o menus.ppc.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw dialogs.68k.rdump -o dialogs.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw environment.68k.rdump -o environment.68k.rsrc

docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw tracking.68k.rdump -o tracking.68k.rsrc

docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw menu_tracking.68k.rdump -o menu_tracking.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw menu_tracking.ppc.rdump -o menu_tracking.ppc.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw working.68k.rdump -o working.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw standard_file.68k.rdump -o standard_file.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw open_file.68k.rdump -o open_file.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw persistence.68k.rdump -o persistence.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw sound_channels.68k.rdump -o sound_channels.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw devices.68k.rdump -o devices.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw color_ports.68k.rdump -o color_ports.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw color_windows.68k.rdump -o color_windows.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw sampled_sound.68k.rdump -o sampled_sound.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw handle_state.68k.rdump -o handle_state.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw queued_sound.68k.rdump -o queued_sound.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw resource_sound.68k.rdump -o resource_sound.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw sound_info.68k.rdump -o sound_info.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw timers.68k.rdump -o timers.68k.rsrc
printf 'Generated application forks in %s\n' "${output_dir}"
