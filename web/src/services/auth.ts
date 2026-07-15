import { createSignal } from "solid-js";
import { getSetting } from "./settings";
import { storeFetch } from "./store";

const [authed, setAuthed] = createSignal(false);

export function isAuthed(): boolean {
  return authed();
}

export function setUnauthed(): void {
  setAuthed(false);
}

/** GETs /stats and sets the signal from the result; an empty token short-circuits to unauthed. */
export async function probeAuth(): Promise<boolean> {
  if (!getSetting("storeToken")) {
    setAuthed(false);
    return false;
  }
  try {
    const res = await storeFetch("/stats");
    setAuthed(res.ok);
    return res.ok;
  } catch {
    setAuthed(false);
    return false;
  }
}
