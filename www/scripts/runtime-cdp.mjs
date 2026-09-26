// Probe transport only; this module is not shipped to the website.
export function connect(webSocketUrl, { WebSocketImpl = WebSocket, timeoutMs = 30_000 } = {}) {
  const ws = new WebSocketImpl(webSocketUrl);
  const pending = new Map();
  const handlers = new Map();
  let nextId = 1;
  let failure = null;
  let rejectReady;
  const ready = new Promise((resolve, reject) => {
    rejectReady = reject;
    ws.addEventListener("open", resolve, { once: true });
  });
  const fail = (error) => {
    failure ??= error;
    rejectReady(failure);
    for (const item of pending.values()) item.reject(failure);
    pending.clear();
  };
  const opening = setTimeout(() => {
    fail(new Error("CDP connection timed out"));
    ws.close();
  }, timeoutMs);
  ready.then(() => clearTimeout(opening), () => clearTimeout(opening));
  ws.addEventListener("close", (event) => fail(new Error(`CDP connection closed (${event.code ?? "unknown"})`)));
  ws.addEventListener("error", () => fail(new Error("CDP connection failed")));
  ws.addEventListener("message", (event) => {
    let message;
    try { message = JSON.parse(event.data); }
    catch (error) { fail(error); ws.close(); return; }
    const item = pending.get(message.id);
    if (item) {
      pending.delete(message.id);
      if (message.error) item.reject(new Error(JSON.stringify(message.error)));
      else item.resolve(message.result ?? {});
    } else {
      for (const handler of handlers.get(message.method) ?? []) {
        Promise.resolve().then(() => handler(message.params ?? {})).catch((error) => {
          fail(error);
          ws.close();
        });
      }
    }
  });
  return {
    ready,
    close() { fail(new Error("CDP client closed")); ws.close(); },
    on(method, handler) {
      if (!handlers.has(method)) handlers.set(method, []);
      handlers.get(method).push(handler);
    },
    send(method, params = {}) {
      if (failure) return Promise.reject(failure);
      const id = nextId++;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`CDP command timed out: ${method}`));
        }, params.timeout == null ? timeoutMs : Math.max(timeoutMs, params.timeout + 1000));
        const item = {
          resolve(value) { clearTimeout(timer); resolve(value); },
          reject(error) { clearTimeout(timer); reject(error); },
        };
        pending.set(id, item);
        try { ws.send(JSON.stringify({ id, method, params })); }
        catch (error) { pending.delete(id); item.reject(error); }
      });
    },
  };
}

function checked(result) {
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result;
}

// A complete diagnostic trace can exceed the CDP/WebSocket message limit.
// Serialize only after sampling ends, keep it by reference in the page, and
// fetch bounded chunks. This leaves every sample and pacing gate intact.
export async function evaluateJson(page, expression, timeout) {
  const remote = checked(await page.send("Runtime.evaluate", {
    expression: `(async () => ({ json: JSON.stringify(await (${expression})) }))()`,
    returnByValue: false,
    awaitPromise: true,
    timeout,
  }));
  if (!remote?.objectId) throw new Error("CDP probe did not return a result object");
  try {
    const call = async (functionDeclaration, args = []) => checked(await page.send("Runtime.callFunctionOn", {
      objectId: remote.objectId, functionDeclaration,
      arguments: args.map(value => ({ value })), returnByValue: true,
    }))?.value;
    const length = await call("function() { return this.json.length; }");
    if (!Number.isSafeInteger(length) || length < 1 || length > 64 * 1024 * 1024) {
      throw new Error(`Invalid diagnostic JSON length: ${length}`);
    }
    const chunks = [];
    for (let offset = 0; offset < length; offset += 250_000) {
      const end = Math.min(length, offset + 250_000);
      const chunk = await call("function(start, end) { return this.json.slice(start, end); }", [offset, end]);
      if (typeof chunk !== "string" || chunk.length !== end - offset) throw new Error("Incomplete diagnostic JSON chunk");
      chunks.push(chunk);
    }
    return JSON.parse(chunks.join(""));
  } finally {
    await page.send("Runtime.releaseObject", { objectId: remote.objectId }).catch(() => {});
  }
}
