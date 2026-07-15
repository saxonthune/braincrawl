import { getSetting } from "./settings";
import { setUnauthed } from "./auth";

function storeUrl(path: string): string {
  const base = getSetting("storeBaseUrl").replace(/\/$/, "");
  return `${base}${path}`;
}

function storeHeaders(extra?: Record<string, string>): Record<string, string> {
  const token = getSetting("storeToken");
  const headers: Record<string, string> = { ...extra };
  if (token) headers.Authorization = `Bearer ${token}`;
  return headers;
}

/** Every request to the store goes through here so auth headers and 401 handling stay in one place. */
export async function storeFetch(path: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(storeUrl(path), {
    ...init,
    headers: { ...storeHeaders(), ...(init?.headers as Record<string, string> | undefined) },
  });
  if (res.status === 401) setUnauthed();
  return res;
}
