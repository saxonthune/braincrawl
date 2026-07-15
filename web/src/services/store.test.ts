import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setSetting } from "./settings";
import { storeFetch } from "./store";

describe("storeFetch", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("prefixes storeBaseUrl and attaches the bearer token when set", async () => {
    setSetting("storeBaseUrl", "https://store.example");
    setSetting("storeToken", "tok123");
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await storeFetch("/stats");

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("https://store.example/stats");
    expect((init.headers as Record<string, string>).Authorization).toBe("Bearer tok123");
  });

  it("omits the Authorization header when no token is set", async () => {
    setSetting("storeBaseUrl", "https://store.example");
    setSetting("storeToken", "");
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await storeFetch("/stats");

    const [, init] = fetchMock.mock.calls[0];
    expect((init.headers as Record<string, string>).Authorization).toBeUndefined();
  });

  it("merges caller-supplied headers and init options", async () => {
    setSetting("storeBaseUrl", "https://store.example");
    setSetting("storeToken", "tok123");
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await storeFetch("/works", { method: "PUT", headers: { "Content-Type": "application/json" } });

    const [, init] = fetchMock.mock.calls[0];
    expect(init.method).toBe("PUT");
    expect((init.headers as Record<string, string>)["Content-Type"]).toBe("application/json");
    expect((init.headers as Record<string, string>).Authorization).toBe("Bearer tok123");
  });

  it("strips a trailing slash from storeBaseUrl", async () => {
    setSetting("storeBaseUrl", "https://store.example/");
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await storeFetch("/stats");

    expect(fetchMock.mock.calls[0][0]).toBe("https://store.example/stats");
  });
});
