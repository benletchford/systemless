#!/bin/sh
set -eu

fixture_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
build_dir="${fixture_dir}/build"
work_dir="${build_dir}/work"
image_name=systemless-toolbox-showcase-mpw
mode=${1:-build}

case "${mode}" in
    build|--update|--verify) ;;
    *)
        printf '%s\n' "usage: $0 [--update|--verify]" >&2
        exit 2
        ;;
esac

docker build --tag "${image_name}" --file "${fixture_dir}/Dockerfile" "${fixture_dir}"

rm -rf "${build_dir}"
mkdir -p "${work_dir}"
cp "${fixture_dir}/build.mpw" "${work_dir}/build.mpw"
cp "${fixture_dir}/showcase.c" "${work_dir}/showcase.c"
cp "${fixture_dir}/showcase.r" "${work_dir}/showcase.r"

docker run --rm \
    --volume "${work_dir}:/workspace" \
    "${image_name}" \
    %%% /workspace/build.mpw

test -s "${work_dir}/showcase"
test -s "${work_dir}/showcase.rdump"

docker run --rm \
    --volume "${work_dir}:/workspace" \
    --entrypoint SimpleRez \
    "${image_name}" \
    showcase.rdump -o showcase.rsrc

cp "${work_dir}/showcase" "${build_dir}/showcase"
cp "${work_dir}/showcase.rsrc" "${build_dir}/showcase.rsrc"

cargo run --quiet --locked \
    --manifest-path "${fixture_dir}/../../Cargo.toml" \
    --example toolbox_showcase_packer \
    -- \
    "${build_dir}/showcase" \
    "${build_dir}/showcase.rsrc" \
    "${build_dir}/toolbox-showcase.sit"

if [ "${mode}" = "--update" ]; then
    cp "${build_dir}/toolbox-showcase.sit" "${fixture_dir}/toolbox-showcase.sit"
elif [ "${mode}" = "--verify" ]; then
    cmp "${build_dir}/toolbox-showcase.sit" "${fixture_dir}/toolbox-showcase.sit"
fi

printf '%s\n' "Built ${build_dir}/toolbox-showcase.sit"
