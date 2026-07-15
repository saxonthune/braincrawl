import { createSignal, type JSX } from "solid-js";
import { getAllSettings, setSetting } from "./settings";

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
  const [storeToken, setStoreToken] = createSignal(initial.storeToken);
  const [storeBaseUrl, setStoreBaseUrl] = createSignal(initial.storeBaseUrl);
  const [model, setModel] = createSignal(initial.model);

  return (
    <div class="chat-settings">
      <p>
        <a href="#/chat">&larr; Sessions</a>
      </p>
      <h2>Settings</h2>
      <p class="chat-settings-note">
        These values live only in this browser's localStorage — they are never sent anywhere
        except the requests you make from here.
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
        <PasswordField
          label="Store token"
          value={storeToken()}
          onChange={(v) => {
            setStoreToken(v);
            setSetting("storeToken", v);
          }}
        />
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
          <span class="chat-settings-note">
            Anthropic key: a model id like claude-opus-4-8. OpenRouter key: a slug like
            anthropic/claude-sonnet-4.6 (Anthropic models only on this endpoint).
          </span>
        </label>
      </form>
    </div>
  );
}
