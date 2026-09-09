import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { isAuthed, probeAuth, setUnauthed } from "./auth";
import { setSetting } from "./settings";

describe("auth signal", () => {
  beforeEach(() => {
    localStorage.clear();
    setUnauthed();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("probes tokenless and authenticates against an auth-disabled backend", async () => {
    setSetting("storeToken", "");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 200 })));

    const result = await probeAuth();

    expect(result).toBe(true);
    expect(isAuthed()).toBe(true);
  });

  it("becomes authed on a 200 /stats response", async () => {
    setSetting("storeToken", "tok123");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 200 })));

    const result = await probeAuth();

    expect(result).toBe(true);
    expect(isAuthed()).toBe(true);
  });

  it("becomes unauthed on a 401 /stats response", async () => {
    setSetting("storeToken", "tok123");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 401 })));

    const result = await probeAuth();

    expect(result).toBe(false);
    expect(isAuthed()).toBe(false);
  });

  it("setUnauthed flips the signal back off", async () => {
    setSetting("storeToken", "tok123");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 200 })));
    await probeAuth();
    expect(isAuthed()).toBe(true);

    setUnauthed();

    expect(isAuthed()).toBe(false);
  });
});
