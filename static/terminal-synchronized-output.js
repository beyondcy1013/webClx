(function attachSynchronizedOutput(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.WebClxTerminalSynchronizedOutput = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function createSynchronizedOutput() {
  const START = Uint8Array.of(0x1b, 0x5b, 0x3f, 0x32, 0x30, 0x32, 0x36, 0x68);
  const END = Uint8Array.of(0x1b, 0x5b, 0x3f, 0x32, 0x30, 0x32, 0x36, 0x6c);
  const MAX_PENDING_BYTES = 1024 * 1024;

  function concatBytes(chunks) {
    const usable = chunks.filter((chunk) => chunk instanceof Uint8Array && chunk.length > 0);
    if (usable.length === 0) {
      return new Uint8Array();
    }
    if (usable.length === 1) {
      return usable[0];
    }
    const result = new Uint8Array(usable.reduce((total, chunk) => total + chunk.length, 0));
    let offset = 0;
    usable.forEach((chunk) => {
      result.set(chunk, offset);
      offset += chunk.length;
    });
    return result;
  }

  function findSequence(source, target, start = 0) {
    const limit = source.length - target.length;
    for (let index = Math.max(start, 0); index <= limit; index += 1) {
      let matches = true;
      for (let offset = 0; offset < target.length; offset += 1) {
        if (source[index + offset] !== target[offset]) {
          matches = false;
          break;
        }
      }
      if (matches) {
        return index;
      }
    }
    return -1;
  }

  function partialSequenceTailLength(source, target) {
    const maxLength = Math.min(source.length, target.length - 1);
    for (let length = maxLength; length > 0; length -= 1) {
      let matches = true;
      for (let offset = 0; offset < length; offset += 1) {
        if (source[source.length - length + offset] !== target[offset]) {
          matches = false;
          break;
        }
      }
      if (matches) {
        return length;
      }
    }
    return 0;
  }

  function createSynchronizedOutputTransformer() {
    let pending = new Uint8Array();
    let synchronized = false;

    function transform(bytes) {
      if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
        return new Uint8Array();
      }

      let buffer = concatBytes([pending, bytes]);
      pending = new Uint8Array();
      const emitted = [];

      while (buffer.length > 0) {
        if (synchronized) {
          const endIndex = findSequence(buffer, END, START.length);
          if (endIndex < 0) {
            pending = buffer;
            if (pending.length > MAX_PENDING_BYTES) {
              emitted.push(pending);
              pending = new Uint8Array();
              synchronized = false;
            }
            break;
          }
          const endOffset = endIndex + END.length;
          emitted.push(buffer.slice(0, endOffset));
          buffer = buffer.slice(endOffset);
          synchronized = false;
          continue;
        }

        const startIndex = findSequence(buffer, START);
        if (startIndex >= 0) {
          if (startIndex > 0) {
            emitted.push(buffer.slice(0, startIndex));
          }
          buffer = buffer.slice(startIndex);
          synchronized = true;
          continue;
        }

        const tailLength = partialSequenceTailLength(buffer, START);
        const emitLength = buffer.length - tailLength;
        if (emitLength > 0) {
          emitted.push(buffer.slice(0, emitLength));
        }
        pending = tailLength > 0 ? buffer.slice(emitLength) : new Uint8Array();
        break;
      }

      return concatBytes(emitted);
    }

    return {
      transform,
      flush() {
        const output = pending;
        pending = new Uint8Array();
        synchronized = false;
        return output;
      },
      reset() {
        pending = new Uint8Array();
        synchronized = false;
      },
    };
  }

  return { createSynchronizedOutputTransformer };
});
