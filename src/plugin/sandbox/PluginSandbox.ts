import type { Permission, PluginManifest } from "../types";

interface SandboxOptions {
  pluginId: string;
  permissions: Permission[];
  manifest: PluginManifest;
}

interface SandboxMessage {
  type: "api-call" | "invoke-response" | "log" | "ready" | "error";
  payload: unknown;
}

interface APICallRequest {
  api: string;
  method: string;
  args: unknown[];
  callId: string;
}

export class PluginSandbox {
  private worker: Worker;
  private pluginId: string;
  private permissions: Set<string>;
  private pendingCalls: Map<string, { resolve: Function; reject: Function }>;
  private onLog?: (level: string, message: string) => void;

  constructor(options: SandboxOptions) {
    this.pluginId = options.pluginId;
    this.permissions = new Set(options.permissions.map((p) => p.permission));
    this.pendingCalls = new Map();

    const blob = new Blob([this.getWorkerCode()], {
      type: "application/javascript",
    });
    this.worker = new Worker(URL.createObjectURL(blob));

    this.worker.onmessage = this.handleMessage.bind(this);
    this.worker.onerror = this.handleError.bind(this);
  }

  async loadScript(scriptContent: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const handler = (e: MessageEvent) => {
        if (e.data.type === "ready") {
          this.worker.removeEventListener("message", handler);
          resolve();
        } else if (e.data.type === "error") {
          this.worker.removeEventListener("message", handler);
          reject(new Error(e.data.payload));
        }
      };
      this.worker.addEventListener("message", handler);

      this.worker.postMessage({
        type: "load",
        payload: {
          script: scriptContent,
          pluginId: this.pluginId,
        },
      });
    });
  }

  async invoke(method: string, ...args: unknown[]): Promise<unknown> {
    const callId = `${this.pluginId}-${Date.now()}-${Math.random()}`;

    return new Promise((resolve, reject) => {
      this.pendingCalls.set(callId, { resolve, reject });

      this.worker.postMessage({
        type: "invoke",
        payload: { method, args, callId },
      });

      setTimeout(() => {
        if (this.pendingCalls.has(callId)) {
          this.pendingCalls.delete(callId);
          reject(new Error("Plugin call timeout"));
        }
      }, 30000);
    });
  }

  destroy(): void {
    this.worker.terminate();
    this.pendingCalls.forEach(({ reject }) => {
      reject(new Error("Sandbox destroyed"));
    });
    this.pendingCalls.clear();
  }

  setLogHandler(handler: (level: string, message: string) => void): void {
    this.onLog = handler;
  }

  private handleMessage(e: MessageEvent<SandboxMessage>): void {
    const { type, payload } = e.data;

    switch (type) {
      case "api-call":
        this.handleAPICall(payload as APICallRequest);
        break;
      case "log":
        if (this.onLog) {
          const { level, message } = payload as {
            level: string;
            message: string;
          };
          this.onLog(level, message);
        }
        break;
      case "invoke-response": {
        const { callId, result, error } = payload as {
          callId: string;
          result?: unknown;
          error?: string;
        };
        const pending = this.pendingCalls.get(callId);
        if (pending) {
          this.pendingCalls.delete(callId);
          error ? pending.reject(new Error(error)) : pending.resolve(result);
        }
        break;
      }
      case "error":
        console.error(`[PluginSandbox:${this.pluginId}]`, payload);
        break;
    }
  }

  private handleError(e: ErrorEvent): void {
    console.error(`[PluginSandbox:${this.pluginId}] Worker error:`, e);
  }

  private async handleAPICall(request: APICallRequest): Promise<void> {
    const { api, method, args, callId } = request;

    try {
      const permission = `${api}:${method}`;
      if (!this.hasPermission(permission)) {
        throw new Error(`Permission denied: ${permission}`);
      }

      const result = await this.callRealAPI(api, method, args);

      this.worker.postMessage({
        type: "api-response",
        payload: { callId, result, error: null },
      });
    } catch (error) {
      this.worker.postMessage({
        type: "api-response",
        payload: { callId, result: null, error: String(error) },
      });
    }
  }

  private hasPermission(permission: string): boolean {
    return (
      this.permissions.has(permission) ||
      this.permissions.has(`${permission.split(":")[0]}:*`) ||
      this.permissions.has("*")
    );
  }

  private async callRealAPI(
    api: string,
    method: string,
    args: unknown[],
  ): Promise<unknown> {
    const { clipboard, tabs } = await import("../../lib/tauri-api");

    switch (api) {
      case "clipboard":
        if (method === "getLatest" && this.permissions.has("data:read")) {
          return clipboard.getLatest(args[0] as number);
        }
        break;
      case "tabs":
        if (method === "getAll" && this.permissions.has("data:read")) {
          return tabs.getAll();
        }
        break;
    }

    throw new Error(`Unknown API: ${api}.${method}`);
  }

  private getWorkerCode(): string {
    return `
      'use strict';
      const workerGlobal = self;
      
      // Freeze prototypes to prevent escapes
      (function freezePrototypes() {
        const natives = [Object, Array, Function, String, Number, Boolean, Date, RegExp, Error, Map, Set, Promise];
        natives.forEach(Native => {
          if (Native && Native.prototype) {
            Object.freeze(Native.prototype);
            Object.freeze(Native);
          }
        });
        delete Object.prototype.__proto__;
        delete Object.prototype.constructor;
      })();
      
      // Remove global access
      const globalThis = undefined;
      const window = undefined;
      const document = undefined;
      const fetch = undefined;
      const XMLHttpRequest = undefined;
      const WebSocket = undefined;
      const Worker = undefined;
      const SharedWorker = undefined;
      const ServiceWorker = undefined;
      const indexedDB = undefined;
      const localStorage = undefined;
      const sessionStorage = undefined;
      
      let pluginRuntime = null;
      
      workerGlobal.onmessage = async function(e) {
        const { type, payload } = e.data;
        
        switch (type) {
          case 'load':
            try {
              const sandbox = {
                console: {
                  log: (...args) => workerGlobal.postMessage({ type: 'log', payload: { level: 'info', message: args.join(' ') } }),
                  error: (...args) => workerGlobal.postMessage({ type: 'log', payload: { level: 'error', message: args.join(' ') } }),
                  warn: (...args) => workerGlobal.postMessage({ type: 'log', payload: { level: 'warn', message: args.join(' ') } }),
                },
                Cliporax: {
                  api: {
                    call: async (api, method, ...args) => {
                      return new Promise((resolve, reject) => {
                        const callId = Math.random().toString(36);
                        workerGlobal.postMessage({ type: 'api-call', payload: { api, method, args, callId } });
                        const handler = (e) => {
                          if (e.data.type === 'api-response' && e.data.payload.callId === callId) {
                            workerGlobal.removeEventListener('message', handler);
                            e.data.payload.error 
                              ? reject(new Error(e.data.payload.error))
                              : resolve(e.data.payload.result);
                          }
                        };
                        workerGlobal.addEventListener('message', handler);
                      });
                    },
                  },
                },
              };
              
              Object.freeze(sandbox);
              Object.freeze(sandbox.console);
              Object.freeze(sandbox.Cliporax);
              Object.freeze(sandbox.Cliporax.api);
              
              const safeEval = new Function('sandbox', \`
                with (sandbox) {
                  (function() {
                    'use strict';
                    \${payload.script}
                  })();
                }
              \`);
              
              safeEval(sandbox);
              
              pluginRuntime = sandbox.CliporaxPlugin || {};
              
              workerGlobal.postMessage({ type: 'ready' });
            } catch (error) {
              workerGlobal.postMessage({ type: 'error', payload: error.message });
            }
            break;
            
          case 'invoke':
            if (pluginRuntime && pluginRuntime[payload.method]) {
              try {
                const result = await pluginRuntime[payload.method](...payload.args);
                workerGlobal.postMessage({ type: 'invoke-response', payload: { callId: payload.callId, result } });
              } catch (error) {
                workerGlobal.postMessage({ type: 'invoke-response', payload: { callId: payload.callId, error: error.message } });
              }
            }
            break;
        }
      };
    `;
  }
}
