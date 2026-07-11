export interface PluginFetchOptions extends RequestInit {
  /** Abort the request after this many milliseconds. */
  timeout?: number;
}

export interface PluginNetworkApi {
  fetch(input: RequestInfo | URL, options?: PluginFetchOptions): Promise<Response>;
}

const NETWORK_PERMISSIONS = new Set([
  "network:fetch",
  "network:websocket",
  "network:sync",
]);

function hasNetworkPermission(grantedPermissions: readonly string[]): boolean {
  return grantedPermissions.some((permission) => NETWORK_PERMISSIONS.has(permission));
}

/** Creates the permission-aware network API for one plugin extension. */
export function createPluginNetworkApi(
  pluginId: string,
  grantedPermissions: readonly string[],
): PluginNetworkApi {
  return {
    async fetch(
      input: RequestInfo | URL,
      options: PluginFetchOptions = {},
    ): Promise<Response> {
      if (!hasNetworkPermission(grantedPermissions)) {
        throw new Error(
          `Plugin ${pluginId} requires the network:fetch permission to make network requests.`,
        );
      }

      const { timeout, signal, ...requestOptions } = options;
      if (timeout !== undefined && (!Number.isFinite(timeout) || timeout <= 0)) {
        throw new RangeError("Plugin network request timeout must be a positive number.");
      }

      if (timeout === undefined) {
        return fetch(input, { ...requestOptions, signal });
      }

      const controller = new AbortController();
      const abortFromCaller = () => controller.abort(signal?.reason);
      if (signal?.aborted) {
        abortFromCaller();
      } else {
        signal?.addEventListener("abort", abortFromCaller, { once: true });
      }

      const timeoutId = setTimeout(() => controller.abort(), timeout);
      try {
        return await fetch(input, { ...requestOptions, signal: controller.signal });
      } finally {
        clearTimeout(timeoutId);
        signal?.removeEventListener("abort", abortFromCaller);
      }
    },
  };
}
