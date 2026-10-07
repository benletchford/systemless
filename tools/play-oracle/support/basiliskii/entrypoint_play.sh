#!/bin/bash
set -e

echo "[entrypoint] Starting Xvfb (virtual framebuffer)..."
Xvfb :99 -screen 0 1024x768x24 &
XVFB_PID=$!

# Wait for X server to be ready
sleep 2

export DISPLAY=:99

FFMPEG_PID=""
if [ -n "${BASILISK_PLAY_RECORD_FILE:-}" ]; then
    echo "[entrypoint] Recording Xvfb display to ${BASILISK_PLAY_RECORD_FILE}..."
    ffmpeg -nostdin -y -loglevel error \
        -video_size 800x600 \
        -framerate 60 \
        -f x11grab \
        -i :99+0,0 \
        -c:v libx264 \
        -preset veryfast \
        -pix_fmt yuv420p \
        -movflags +frag_keyframe+empty_moov+default_base_moof \
        "${BASILISK_PLAY_RECORD_FILE}" &
    FFMPEG_PID=$!
fi

echo "[entrypoint] Starting BasiliskII..."
BasiliskII --config /root/.basilisk_ii_prefs &
B_PID=$!
echo "$B_PID" > /tmp/basilisk.pid

echo "[entrypoint] BasiliskII started (PID $B_PID). Container ready for play commands."

# Write a ready signal so the host can detect we're up
touch /tmp/basilisk_ready

cleanup() {
    echo "[entrypoint] Shutting down..."
    kill $B_PID 2>/dev/null || true
    wait $B_PID 2>/dev/null || true
    if [ -n "$FFMPEG_PID" ]; then
        kill -INT "$FFMPEG_PID" 2>/dev/null || true
        wait "$FFMPEG_PID" 2>/dev/null || true
    fi
    kill $XVFB_PID 2>/dev/null || true
    wait $XVFB_PID 2>/dev/null || true
}
trap cleanup EXIT SIGTERM SIGINT

# Block until BasiliskII exits or container is stopped
wait $B_PID 2>/dev/null || true
echo "[entrypoint] BasiliskII exited."
