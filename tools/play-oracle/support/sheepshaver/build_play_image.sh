#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_PATCH_DIR="${SCRIPT_DIR}/macemu-patches"
IMAGE_TAG="sheepshaver-play:latest"
MACEMU_REPO="https://github.com/cebix/macemu"
MACEMU_REF="96e512bd6376e78a2869f16dcc8a9028bce5ee72"
PATCH_DIR="${DEFAULT_PATCH_DIR}"

usage() {
    cat <<EOF
Usage: $(basename "$0") [--patch-dir PATH] [--image TAG] [--macemu-repo URL] [--macemu-ref REF]

Build the SheepShaver play image by cloning macemu inside Docker and applying
host-managed patches copied into a temporary build context. The MacEmu source
defaults to the profile-pinned commit; --macemu-ref intentionally overrides it.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --patch-dir)
            PATCH_DIR="$2"
            shift 2
            ;;
        --image)
            IMAGE_TAG="$2"
            shift 2
            ;;
        --macemu-repo)
            MACEMU_REPO="$2"
            shift 2
            ;;
        --macemu-ref)
            MACEMU_REF="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ ! "${MACEMU_REF}" =~ ^[0-9a-f]{40}$ ]]; then
    echo "MacEmu ref must be a full 40-digit lowercase commit hash" >&2
    exit 1
fi

if [ ! -d "${PATCH_DIR}" ]; then
    echo "Patch directory does not exist: ${PATCH_DIR}" >&2
    exit 1
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

cp "${SCRIPT_DIR}/Dockerfile.play" "${TMP_DIR}/Dockerfile.play"
cp "${SCRIPT_DIR}/entrypoint_play.sh" "${TMP_DIR}/entrypoint_play.sh"
cp "${SCRIPT_DIR}/.sheepshaver_prefs.play" "${TMP_DIR}/.sheepshaver_prefs.play"
cp "${SCRIPT_DIR}/apply_macemu_patches.sh" "${TMP_DIR}/apply_macemu_patches.sh"
mkdir -p "${TMP_DIR}/macemu-patches"
cp -R "${PATCH_DIR}/." "${TMP_DIR}/macemu-patches/" 2>/dev/null || true

BUILD_ARGS=(
    --build-arg "MACEMU_REPO=${MACEMU_REPO}"
)
if [ -n "${MACEMU_REF}" ]; then
    BUILD_ARGS+=(--build-arg "MACEMU_REF=${MACEMU_REF}")
fi

docker build \
    -f "${TMP_DIR}/Dockerfile.play" \
    -t "${IMAGE_TAG}" \
    "${BUILD_ARGS[@]}" \
    "${TMP_DIR}"
