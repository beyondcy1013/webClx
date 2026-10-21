// Keep older Android WebViews compatible with request cancellation helpers.
(function installAbortSignalCompat(global) {
  const signalType = global.AbortSignal;
  const controllerType = global.AbortController;
  if (!signalType || !controllerType) {
    return;
  }

  if (typeof signalType.any !== "function") {
    signalType.any = (signals) => {
      const controller = new controllerType();
      for (const signal of Array.from(signals || [])) {
        if (!signal || typeof signal.addEventListener !== "function") {
          continue;
        }
        const abort = () => {
          if (controller.signal.aborted) {
            return;
          }
          try {
            controller.abort(signal.reason);
          } catch (_) {
            controller.abort();
          }
        };
        if (signal.aborted) {
          abort();
          break;
        }
        signal.addEventListener("abort", abort, { once: true });
      }
      return controller.signal;
    };
  }

  if (typeof signalType.timeout !== "function") {
    signalType.timeout = (milliseconds) => {
      const controller = new controllerType();
      global.setTimeout(() => {
        try {
          controller.abort(new DOMException("The operation timed out.", "TimeoutError"));
        } catch (_) {
          controller.abort();
        }
      }, milliseconds);
      return controller.signal;
    };
  }
})(window);
