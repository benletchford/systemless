#!/bin/sh
# Uses the repository's pinned MPW image; see tests/toolbox-showcase/Dockerfile.
set -eu
fixture_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output_dir="${fixture_dir}/../../target/mpw-probe"
mkdir -p "${output_dir}"
output_dir=$(CDPATH= cd -- "${output_dir}" && pwd)
cp "${fixture_dir}/probe.c" "${fixture_dir}/probe.r" "${fixture_dir}/build.mpw" "${output_dir}/"
docker run --rm --volume "${output_dir}:/workspace" systemless-toolbox-showcase-mpw %%% /workspace/build.mpw
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw probe.68k.rdump -o probe.68k.rsrc
docker run --rm --volume "${output_dir}:/workspace" --entrypoint SimpleRez systemless-toolbox-showcase-mpw probe.ppc.rdump -o probe.ppc.rsrc
printf 'Generated application forks in %s\n' "${output_dir}"
