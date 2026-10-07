#!/bin/bash
set -euo pipefail

MACEMU_DIR="${1:-/opt/macemu}"
PATCH_DIR="${2:-/opt/macemu-patches}"

if [ ! -d "${PATCH_DIR}" ]; then
    echo "[apply_macemu_patches] No patch directory found at ${PATCH_DIR}; skipping."
    exit 0
fi

shopt -s nullglob
PATCH_FILES=("${PATCH_DIR}"/*.patch "${PATCH_DIR}"/*.diff)
shopt -u nullglob

if [ ${#PATCH_FILES[@]} -eq 0 ]; then
    echo "[apply_macemu_patches] No patch files found in ${PATCH_DIR}; skipping."
    exit 0
fi

IFS=$'\n' SORTED_PATCHES=($(sort <<<"${PATCH_FILES[*]}"))
unset IFS

echo "[apply_macemu_patches] Applying ${#SORTED_PATCHES[@]} patch(es) from ${PATCH_DIR} to ${MACEMU_DIR}..."

for patch_file in "${SORTED_PATCHES[@]}"; do
    patch_name="$(basename "${patch_file}")"
    echo "[apply_macemu_patches]   Applying ${patch_name}..."
    git -C "${MACEMU_DIR}" apply --verbose --recount "${patch_file}"
done

echo "[apply_macemu_patches] All patches applied successfully."
