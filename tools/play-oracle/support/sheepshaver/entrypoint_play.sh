#!/bin/bash
set -e

# Allow low-memory mapping for SheepShaver's real addressing mode
sysctl -w vm.mmap_min_addr=0 2>/dev/null || true

TARGET_HOME="${HOME:-/tmp}"
if [ -f /root/.sheepshaver_prefs ]; then
    cp /root/.sheepshaver_prefs "${TARGET_HOME}/.sheepshaver_prefs" 2>/dev/null || true
fi

# Match the virtual X display to the configured guest display. Legacy
# DrawSprocket games switch SheepShaver into an exact-size fullscreen window;
# a larger X root leaves that SDL mode black under Xvfb.
SCREEN_SPEC=$(awk '$1 == "screen" { print $2; exit }' "${TARGET_HOME}/.sheepshaver_prefs")
IFS=/ read -r SCREEN_MODE SCREEN_WIDTH SCREEN_HEIGHT <<EOF
${SCREEN_SPEC}
EOF
case "${SCREEN_WIDTH}x${SCREEN_HEIGHT}" in
    *[!0-9x]*|x) SCREEN_WIDTH=800; SCREEN_HEIGHT=600 ;;
esac

echo "[entrypoint] Starting Xvfb (${SCREEN_WIDTH}x${SCREEN_HEIGHT}x24)..."
Xvfb :99 -screen 0 "${SCREEN_WIDTH}x${SCREEN_HEIGHT}x24" &
XVFB_PID=$!

# Wait for X server to be ready
sleep 2

export DISPLAY=:99

FFMPEG_PID=""
if [ -n "${SHEEPSHAVER_PLAY_RECORD_FILE:-}" ]; then
    echo "[entrypoint] Recording Xvfb display to ${SHEEPSHAVER_PLAY_RECORD_FILE}..."
    ffmpeg -nostdin -y -loglevel error \
        -video_size 800x600 \
        -framerate 60 \
        -f x11grab \
        -i :99+0,0 \
        -c:v libx264 \
        -preset veryfast \
        -pix_fmt yuv420p \
        -movflags +frag_keyframe+empty_moov+default_base_moof \
        "${SHEEPSHAVER_PLAY_RECORD_FILE}" &
    FFMPEG_PID=$!
fi

echo "[entrypoint] Starting SheepShaver..."
HOME="${TARGET_HOME}" SheepShaver 2>&1 | tee /tmp/sheepshaver.log &
S_PID=$!
echo "$S_PID" > /tmp/sheepshaver.pid

echo "[entrypoint] SheepShaver started (PID $S_PID). Container ready for play commands."

touch /tmp/sheepshaver_ready

cleanup() {
    echo "[entrypoint] Shutting down..."
    kill $S_PID 2>/dev/null || true
    wait $S_PID 2>/dev/null || true
    if [ -n "$FFMPEG_PID" ]; then
        kill -INT "$FFMPEG_PID" 2>/dev/null || true
        wait "$FFMPEG_PID" 2>/dev/null || true
    fi
    kill $XVFB_PID 2>/dev/null || true
    wait $XVFB_PID 2>/dev/null || true
}
trap cleanup EXIT SIGTERM SIGINT

# Block until SheepShaver exits or container is stopped
wait $S_PID 2>/dev/null || true
echo "[entrypoint] SheepShaver exited."
