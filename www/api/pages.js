import RustWorker from "./rust.js";

// Pages expects an ExportedHandler object; worker-build emits a WorkerEntrypoint.
export default {
  fetch(request, env, context) {
    return new RustWorker(context, env).fetch(request);
  },
};
