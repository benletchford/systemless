#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SITE_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
REPOSITORY_DIR=$(CDPATH= cd -- "$SITE_DIR/.." && pwd)
DIST_DIR=${SYSTEMLESS_PAGES_DIST:-"$SITE_DIR/dist"}
TRUNK_VERSION=${TRUNK_VERSION:-0.21.14}

TASK_CARGO_HOME=${CARGO_HOME:-"${HOME:?HOME must be set}/.cargo"}
PATH="$TASK_CARGO_HOME/bin:$PATH"
export PATH

if ! command -v rustup >/dev/null 2>&1; then
  if ! command -v curl >/dev/null 2>&1; then
    echo "rustup is not installed and curl is unavailable; install Rust before building." >&2
    exit 1
  fi

  echo "rustup not found; installing a minimal stable Rust toolchain."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable
fi

rustup target add wasm32-unknown-unknown

install_trunk() {
  if command -v trunk >/dev/null 2>&1 \
    && [ "$(trunk --version 2>/dev/null)" = "trunk $TRUNK_VERSION" ]; then
    return
  fi

  case "$(uname -s):$(uname -m)" in
    Linux:x86_64) TRUNK_TARGET=x86_64-unknown-linux-gnu ;;
    Linux:aarch64|Linux:arm64) TRUNK_TARGET=aarch64-unknown-linux-gnu ;;
    Darwin:x86_64) TRUNK_TARGET=x86_64-apple-darwin ;;
    Darwin:arm64|Darwin:aarch64) TRUNK_TARGET=aarch64-apple-darwin ;;
    *)
      echo "No prebuilt Trunk archive for this platform; installing from source."
      cargo install trunk --locked --version "$TRUNK_VERSION"
      return
      ;;
  esac

  if ! command -v curl >/dev/null 2>&1 || ! command -v tar >/dev/null 2>&1; then
    echo "curl and tar are required to install the pinned Trunk release." >&2
    exit 1
  fi

  TRUNK_ARCHIVE="trunk-$TRUNK_TARGET.tar.gz"
  TRUNK_RELEASE_URL="https://github.com/trunk-rs/trunk/releases/download/v$TRUNK_VERSION"
  TRUNK_TEMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/systemless-trunk.XXXXXX")
  cleanup_trunk_temp() {
    rm -rf -- "$TRUNK_TEMP_DIR"
  }
  trap cleanup_trunk_temp EXIT HUP INT TERM

  curl --proto '=https' --tlsv1.2 -fsSL \
    "$TRUNK_RELEASE_URL/$TRUNK_ARCHIVE" \
    -o "$TRUNK_TEMP_DIR/$TRUNK_ARCHIVE"
  curl --proto '=https' --tlsv1.2 -fsSL \
    "$TRUNK_RELEASE_URL/$TRUNK_ARCHIVE.sha256" \
    -o "$TRUNK_TEMP_DIR/$TRUNK_ARCHIVE.sha256"

  TRUNK_EXPECTED_SHA256=$(tr -d '[:space:]' < "$TRUNK_TEMP_DIR/$TRUNK_ARCHIVE.sha256")
  if command -v sha256sum >/dev/null 2>&1; then
    TRUNK_ACTUAL_SHA256=$(sha256sum "$TRUNK_TEMP_DIR/$TRUNK_ARCHIVE" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    TRUNK_ACTUAL_SHA256=$(shasum -a 256 "$TRUNK_TEMP_DIR/$TRUNK_ARCHIVE" | awk '{print $1}')
  else
    echo "sha256sum or shasum is required to verify the Trunk archive." >&2
    exit 1
  fi
  if [ "$TRUNK_ACTUAL_SHA256" != "$TRUNK_EXPECTED_SHA256" ]; then
    echo "Trunk archive checksum mismatch." >&2
    exit 1
  fi

  tar -xzf "$TRUNK_TEMP_DIR/$TRUNK_ARCHIVE" -C "$TRUNK_TEMP_DIR"
  mkdir -p "$TASK_CARGO_HOME/bin"
  cp "$TRUNK_TEMP_DIR/trunk" "$TASK_CARGO_HOME/bin/trunk"
  chmod 0755 "$TASK_CARGO_HOME/bin/trunk"
  cleanup_trunk_temp
  trap - EXIT HUP INT TERM
}

install_trunk

# Trunk 0.21 rejects common NO_COLOR values like "1" for release builds.
NO_COLOR=false
export NO_COLOR

# RUSTFLAGS overrides .cargo/config.toml, so preserve the worker stack size here.
SYSTEMLESS_WASM_RUSTFLAGS="-C target-feature=+simd128 -C link-arg=-zstack-size=4194304"
if [ -n "${RUSTFLAGS:-}" ]; then
  RUSTFLAGS="$RUSTFLAGS $SYSTEMLESS_WASM_RUSTFLAGS"
else
  RUSTFLAGS="$SYSTEMLESS_WASM_RUSTFLAGS"
fi
export RUSTFLAGS

(cd "$SITE_DIR" && trunk build --release --locked --public-url / --dist "$DIST_DIR")

RUSTFLAGS= cargo run --locked --manifest-path "$REPOSITORY_DIR/Cargo.toml" \
  -p systemless-catalogue-tools --bin catalogue -- \
  --root "$SITE_DIR" pages --template "$DIST_DIR/index.html" \
  --output "$DIST_DIR" --origin "${SYSTEMLESS_SITE_ORIGIN:-https://systemless.org}"
echo "Built Cloudflare Pages output at $DIST_DIR"
