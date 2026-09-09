import { createSignal, For, type JSX } from "solid-js";
import { getAllSettings, setSetting } from "../../services/settings";

// OpenRouter slugs (proxy mode / sk-or- keys); "opus (direct)" is the plain
// Anthropic id for a pasted sk-ant- key.
const MODEL_PRESETS = [
  { label: "sonnet", slug: "~anthropic/claude-sonnet-latest" },
  { label: "sonnet 4.6", slug: "anthropic/claude-sonnet-4.6" },
  { label: "deepseek v4 flash", slug: "deepseek/deepseek-v4-flash" },
  { label: "opus (direct)", slug: "claude-opus-4-8" },
];

function PasswordField(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}): JSX.Element {
  const [visible, setVisible] = createSignal(false);
  return (
    <label class="chat-settings-field">
      {props.label}
      <div class="chat-settings-password-row">
        <input
          type={visible() ? "text" : "password"}
          value={props.value}
          onInput={(e) => props.onChange(e.currentTarget.value)}
        />
        <button type="button" onClick={() => setVisible((v) => !v)}>
          {visible() ? "Hide" : "Show"}
        </button>
      </div>
    </label>
  );
}

export function Settings(): JSX.Element {
  const initial = getAllSettings();
  const [anthropicKey, setAnthropicKey] = createSignal(initial.anthropicKey);
  const [storeBaseUrl, setStoreBaseUrl] = createSignal(initial.storeBaseUrl);
  const [model, setModel] = createSignal(initial.model);

  return (
    <div class="chat-settings">
      <p>
        <a href="#/chat">&larr; Sessions</a>
      </p>
      <h2>Settings</h2>
      <p class="chat-settings-note">
        These values live only in this browser's localStorage — they are never sent anywhere except
        the requests you make from here.
      </p>
      <form>
        <PasswordField
          label="API key (Anthropic sk-ant-… or OpenRouter sk-or-…)"
          value={anthropicKey()}
          onChange={(v) => {
            setAnthropicKey(v);
            setSetting("anthropicKey", v);
          }}
        />
        <span class="chat-settings-note">
          Optional — leave empty to use the server's key via the LLM proxy. A pasted key overrides
          the proxy and talks to Anthropic or OpenRouter directly.
        </span>
        <label class="chat-settings-field">
          Store base URL
          <input
            type="text"
            value={storeBaseUrl()}
            placeholder="(same origin)"
            onInput={(e) => {
              setStoreBaseUrl(e.currentTarget.value);
              setSetting("storeBaseUrl", e.currentTarget.value);
            }}
          />
        </label>
        <label class="chat-settings-field">
          Model
          <input
            type="text"
            value={model()}
            onInput={(e) => {
              setModel(e.currentTarget.value);
              setSetting("model", e.currentTarget.value);
            }}
          />
          <div class="chat-settings-presets">
            <For each={MODEL_PRESETS}>
              {(preset) => (
                <button
                  type="button"
                  classList={{ active: model() === preset.slug }}
                  onClick={() => {
                    setModel(preset.slug);
                    setSetting("model", preset.slug);
                  }}
                >
                  {preset.label}
                </button>
              )}
            </For>
          </div>
          <span class="chat-settings-note">
            Proxy mode (empty key) and OpenRouter key: any OpenRouter slug —
            anthropic/claude-sonnet-4.6, ~anthropic/claude-sonnet-latest, or a non-Anthropic model
            like deepseek/deepseek-v4-flash. A pasted Anthropic sk-ant- key needs a plain Anthropic
            model id instead, like claude-opus-4-8.
          </span>
        </label>
      </form>
    </div>
  );
}
