import { createSignal, Show, type JSX } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { probeAuth } from "../../services/auth";
import { setSetting } from "../../services/settings";
import { storeFetch } from "../../services/store";
import type { Plugin } from "../types";

type Step = "email" | "code";

function AuthPage(): JSX.Element {
  const navigate = useNavigate();
  const [step, setStep] = createSignal<Step>("email");
  const [email, setEmail] = createSignal("");
  const [code, setCode] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const requestCode = async (e: Event) => {
    e.preventDefault();
    if (!email().trim() || busy()) return;
    setBusy(true);
    try {
      await storeFetch("/api/auth/request-code", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: email().trim() }),
      });
    } finally {
      setBusy(false);
      setStep("code");
    }
  };

  const verify = async (e: Event) => {
    e.preventDefault();
    if (!code().trim() || busy()) return;
    setBusy(true);
    setError("");
    try {
      const res = await storeFetch("/api/auth/verify", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: email().trim(), code: code().trim() }),
      });
      if (!res.ok) {
        setError("invalid or expired code");
        return;
      }
      const json = (await res.json()) as { token: string };
      setSetting("storeToken", json.token);
      await probeAuth();
      navigate("/");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="auth-page">
      <h2>Sign in</h2>
      <Show when={step() === "email"}>
        <form onSubmit={(e) => void requestCode(e)}>
          <label>
            Email
            <input
              type="email"
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
              autofocus
            />
          </label>
          <button type="submit" disabled={busy() || !email().trim()}>
            Send code
          </button>
        </form>
      </Show>
      <Show when={step() === "code"}>
        <p>If that address is allowed, a code was sent.</p>
        <form onSubmit={(e) => void verify(e)}>
          <label>
            Code
            <input type="text" value={code()} onInput={(e) => setCode(e.currentTarget.value)} autofocus />
          </label>
          <button type="submit" disabled={busy() || !code().trim()}>
            Verify
          </button>
        </form>
        <Show when={error()}>
          <p class="auth-error">{error()}</p>
        </Show>
      </Show>
      <p>
        <a href="#/settings">Have a token? Paste it instead.</a>
      </p>
    </div>
  );
}

export const authPlugin: Plugin = {
  id: "auth",
  routes: [{ path: "/auth", component: AuthPage }],
};
