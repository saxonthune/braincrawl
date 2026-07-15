import { createSignal } from "solid-js";
import { storeFetch } from "./store";

const [authed, setAuthed] = createSignal(false);

export function isAuthed(): boolean {
  return authed();
}

export function setUnauthed(): void {
  setAuthed(false);
}

/**
 * GETs /stats and sets the signal from the result. The probe is the sole
 * authority — no token short-circuit — so an auth-disabled backend (local
 * dev server) authenticates a tokenless browser.
 */
export async function probeAuth(): Promise<boolean> {
  try {
    const res = await storeFetch("/stats");
    setAuthed(res.ok);
    return res.ok;
  } catch {
    setAuthed(false);
    return false;
  }
}
