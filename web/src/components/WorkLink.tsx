import { Show, type JSX } from "solid-js";

/** Renders a catalog endpoint (`openalex:…`, `doi:…`) as an outbound link,
 * or plain text when it has no known home. */
export function WorkLink(props: { target: string }): JSX.Element {
  const href = () => {
    if (props.target.startsWith("openalex:"))
      return `https://openalex.org/${props.target.slice("openalex:".length)}`;
    if (props.target.startsWith("doi:"))
      return `https://doi.org/${props.target.slice("doi:".length)}`;
    return null;
  };
  return (
    <Show when={href()} fallback={<span>{props.target}</span>}>
      {(url) => (
        <a href={url()} target="_blank" rel="noreferrer">
          {props.target}
        </a>
      )}
    </Show>
  );
}
