export interface ChatSettings {
  anthropicKey: string;
  storeToken: string;
  storeBaseUrl: string;
  model: string;
}

const KEYS = {
  anthropicKey: "bc.settings.anthropicKey",
  storeToken: "bc.settings.storeToken",
  storeBaseUrl: "bc.settings.storeBaseUrl",
  model: "bc.settings.model",
} as const;

const DEFAULTS: ChatSettings = {
  anthropicKey: "",
  storeToken: "",
  storeBaseUrl: "",
  model: "claude-opus-4-8",
};

function getItem(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch (err) {
    console.warn(`chat settings: failed to read ${key}`, err);
    return fallback;
  }
}

function setItem(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch (err) {
    console.warn(`chat settings: failed to persist ${key}`, err);
  }
}

export function getSetting<K extends keyof ChatSettings>(key: K): ChatSettings[K] {
  return getItem(KEYS[key], DEFAULTS[key]);
}

export function setSetting<K extends keyof ChatSettings>(key: K, value: ChatSettings[K]): void {
  setItem(KEYS[key], value);
}

export function getAllSettings(): ChatSettings {
  return {
    anthropicKey: getSetting("anthropicKey"),
    storeToken: getSetting("storeToken"),
    storeBaseUrl: getSetting("storeBaseUrl"),
    model: getSetting("model"),
  };
}
