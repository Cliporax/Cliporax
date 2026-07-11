import { afterEach, describe, expect, it, vi } from "vitest";
import { createPluginNetworkApi } from "./network";

describe("createPluginNetworkApi", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("rejects requests when the plugin has no network permission", async () => {
    const network = createPluginNetworkApi("com.example.plugin", []);

    await expect(network.fetch("https://example.com")).rejects.toThrow(
      "network:fetch permission",
    );
  });

  it("forwards a permitted request to the web fetch API", async () => {
    const response = new Response("ok");
    const request = vi.fn().mockResolvedValue(response);
    vi.stubGlobal("fetch", request);
    const network = createPluginNetworkApi("com.example.plugin", [
      "network:fetch",
    ]);

    await expect(
      network.fetch("https://example.com/api", {
        method: "POST",
        body: "payload",
      }),
    ).resolves.toBe(response);

    expect(request).toHaveBeenCalledWith("https://example.com/api", {
      method: "POST",
      body: "payload",
      signal: undefined,
    });
  });
});
