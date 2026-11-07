const trackers = new WeakMap();
const sent = new Set();
let identity;

function dailyIdentity(day) {
  if (identity?.day === day) return identity;
  try {
    const saved = JSON.parse(localStorage.getItem("systemless.play-identity"));
    if (saved?.day === day && /^[a-f0-9]{32}$/.test(saved.visitor)) return identity = saved;
  } catch {}
  const visitor = Array.from(crypto.getRandomValues(new Uint8Array(16)), b => b.toString(16).padStart(2, "0")).join("");
  identity = { day, visitor };
  try { localStorage.setItem("systemless.play-identity", JSON.stringify(identity)); } catch {}
  return identity;
}

export function playHeartbeat(canvas, running) {
  const tracker = trackers.get(canvas);
  if (tracker) tracker.heartbeat = running ? performance.now() : -Infinity;
}

export function startPlay(canvas, game) {
  // Production only: previews, local tests and automation must not affect rankings.
  if (location.hostname !== "systemless.org" || location.pathname.startsWith("/next")
      || navigator.webdriver || trackers.has(canvas)) return;
  const tracker = { heartbeat: performance.now(), elapsed: 0, interacted: false };
  trackers.set(canvas, tracker);
  let last = performance.now();
  let wasVisible = document.visibilityState === "visible";
  const input = event => {
    if (event.isTrusted && (event.target === canvas
        || event.target.closest?.("[data-mobile-controls], [data-mobile-button-panel]"))) tracker.interacted = true;
  };
  const visibility = () => { last = performance.now(); wasVisible = document.visibilityState === "visible"; };
  document.addEventListener("pointerdown", input, true);
  document.addEventListener("keydown", input, true);
  document.addEventListener("visibilitychange", visibility);
  const stop = () => {
    clearInterval(timer);
    document.removeEventListener("pointerdown", input, true);
    document.removeEventListener("keydown", input, true);
    document.removeEventListener("visibilitychange", visibility);
    trackers.delete(canvas);
  };
  const timer = setInterval(() => {
    if (!canvas.isConnected) { stop(); return; }
    const now = performance.now();
    const visible = document.visibilityState === "visible";
    if (visible && wasVisible && now - tracker.heartbeat < 2000) tracker.elapsed += Math.min(now - last, 1000);
    last = now;
    wasVisible = visible;
    if (tracker.elapsed < 30000 || !tracker.interacted) return;
    stop();
    const day = new Date().toISOString().slice(0, 10);
    const key = `${day}:${game}`;
    if (sent.has(key)) return;
    try { if (localStorage.getItem(`systemless.play.${game}`) === day) return; } catch {}
    sent.add(key);
    const { visitor } = dailyIdentity(day);
    fetch("/api/play", { method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ game, visitor }), keepalive: true }).then(response => {
      if (response.ok) {
        try { localStorage.setItem(`systemless.play.${game}`, day); } catch {}
      } else { sent.delete(key); }
    }).catch(() => sent.delete(key));
  }, 1000);
}
