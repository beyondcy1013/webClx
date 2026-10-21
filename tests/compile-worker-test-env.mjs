export function workerTestEnvironment(overrides = {}) {
  return {
    ...process.env,
    WEBCLX_LOCAL_TOKEN_FILE:
      process.env.WEBCLX_LOCAL_TOKEN_FILE || "/home/bin/webclx/.webclx-local-api-token",
    ...overrides,
  };
}
