#!/bin/bash
set -euo pipefail

MACEMU_DIR="${1:?missing macemu source dir}"
PATCH_DIR="${2:?missing patch dir}"

if [ ! -d "${PATCH_DIR}" ]; then
    echo "[patches] No patch directory at ${PATCH_DIR}; skipping."
    exit 0
fi

mapfile -t PATCHES < <(
    find "${PATCH_DIR}" -maxdepth 1 -type f \( -name '*.patch' -o -name '*.diff' \) | sort
)

if [ "${#PATCHES[@]}" -eq 0 ]; then
    echo "[patches] No .patch or .diff files in ${PATCH_DIR}; skipping."
    exit 0
fi

for patch_file in "${PATCHES[@]}"; do
    echo "[patches] Applying $(basename "${patch_file}")"
    git -C "${MACEMU_DIR}" apply --verbose --recount "${patch_file}"
done
