#!/bin/sh
# Uses the repository's pinned MPW image; see tests/toolbox-showcase/Dockerfile.
set -eu
fixture_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output_dir="${fixture_dir}/../../target/mpw-probe"
mkdir -p "${output_dir}"
output_dir=$(CDPATH= cd -- "${output_dir}" && pwd)
cp "${fixture_dir}/menus.c" "${fixture_dir}/menus.r" "${fixture_dir}/textedit.c" "${fixture_dir}/windows.c" "${fixture_dir}/plugin.c" "${fixture_dir}/dynamic.c" "${fixture_dir}/dynamic.r" "${fixture_dir}/probe.c" "${fixture_dir}/jump.c" "${fixture_dir}/events.c" "${fixture_dir}/wait.c" "${fixture_dir}/draw.c" "${fixture_dir}/probe.r" "${fixture_dir}/build.mpw" "${output_dir}/"
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
printf 'Generated application forks in %s\n' "${output_dir}"
