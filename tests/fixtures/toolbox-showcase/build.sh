#!/bin/sh
set -eu

fixture_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
image=systemless-toolbox-showcase-mpw:1
mode=${1:---check}

case "$mode" in
    --check|--update) ;;
    *)
        echo "usage: $0 [--check|--update]" >&2
        exit 2
        ;;
esac

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/systemless-toolbox-showcase.XXXXXX")
cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$work_dir/src" "$work_dir/packager/src"
cp "$fixture_dir/build.mpw" "$work_dir/build.mpw"
cp "$fixture_dir/src/main.c" "$work_dir/src/main.c"
cp "$fixture_dir/src/showcase.r" "$work_dir/src/showcase.r"
cp "$fixture_dir/src/cfrg.r" "$work_dir/src/cfrg.r"
cp "$fixture_dir/packager/Cargo.toml.in" "$work_dir/packager/Cargo.toml"
cp "$fixture_dir/packager/Cargo.lock" "$work_dir/packager/Cargo.lock"
cp "$fixture_dir/packager/src/main.rs" "$work_dir/packager/src/main.rs"

docker build --tag "$image" "$fixture_dir"
docker run --rm \
    --volume "$work_dir:/workspace" \
    "$image" %%% /workspace/build.mpw
for artifact in \
    main.ppc.o \
    main.68k.o \
    ToolboxShowcase.ppc \
    ToolboxShowcase.68k \
    ToolboxShowcase.68k.rdump \
    ToolboxShowcase \
    ToolboxShowcase.rdump
do
    if [ ! -f "$work_dir/$artifact" ]; then
        echo "MPW did not produce $artifact" >&2
        exit 1
    fi
done
for artifact in \
    main.ppc.o \
    main.68k.o \
    ToolboxShowcase.ppc \
    ToolboxShowcase.68k.rdump \
    ToolboxShowcase \
    ToolboxShowcase.rdump
do
    if [ ! -s "$work_dir/$artifact" ]; then
        echo "MPW produced an empty $artifact" >&2
        exit 1
    fi
done
if ! printf 'Joy!peff' | cmp -n 8 - "$work_dir/ToolboxShowcase" >/dev/null 2>&1; then
    echo "MPW did not produce a PowerPC executable data fork" >&2
    exit 1
fi
docker run --rm \
    --entrypoint SimpleRez \
    --volume "$work_dir:/workspace" \
    "$image" ToolboxShowcase.rdump -o ToolboxShowcase.rsrc
if [ ! -s "$work_dir/ToolboxShowcase.rsrc" ]; then
    echo "SimpleRez did not produce a resource fork" >&2
    exit 1
fi

cargo run --quiet --locked --release \
    --manifest-path "$work_dir/packager/Cargo.toml" -- \
    "$work_dir/ToolboxShowcase" \
    "$work_dir/ToolboxShowcase.rsrc" \
    "$work_dir/ToolboxShowcase.sit"

if [ "$mode" = "--update" ]; then
    cp "$work_dir/ToolboxShowcase.sit" "$fixture_dir/ToolboxShowcase.sit"
    echo "updated $fixture_dir/ToolboxShowcase.sit"
elif ! cmp -s "$work_dir/ToolboxShowcase.sit" "$fixture_dir/ToolboxShowcase.sit"; then
    echo "rebuilt fixture differs from $fixture_dir/ToolboxShowcase.sit" >&2
    exit 1
else
    echo "verified reproducible fixture: $fixture_dir/ToolboxShowcase.sit"
fi
