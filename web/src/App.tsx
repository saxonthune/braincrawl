import { createEffect, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { A, HashRouter, Navigate, Route } from "@solidjs/router";
import { GraphProvider, useGraph } from "./graph";
import { isAuthed, probeAuth } from "./services/auth";
import { plugins } from "./plugins";
import "./App.css";

const [probed, setProbed] = createSignal(false);
void probeAuth().then(() => setProbed(true));

function RequireAuth(props: { children?: JSX.Element }): JSX.Element {
  return (
    <Show when={probed()} fallback={<p>Checking…</p>}>
      <Show when={isAuthed()} fallback={<Navigate href="/auth" />}>
        {props.children}
      </Show>
    </Show>
  );
}

/** Room reserved for the hamburger button (its width plus one gap) when not
 * every nav item fits. */
const BURGER_SPACE = 56;

function Nav(): JSX.Element {
  const { refetch } = useGraph();
  const items = plugins.filter((p) => p.nav).map((p) => p.nav!);
  const [visibleCount, setVisibleCount] = createSignal(items.length);
  const [open, setOpen] = createSignal(false);
  let listRef: HTMLDivElement | undefined;
  let measureRef: HTMLDivElement | undefined;
  let overflowRef: HTMLSpanElement | undefined;

  const recompute = () => {
    if (!listRef || !measureRef) return;
    const available = listRef.clientWidth;
    const gap = parseFloat(getComputedStyle(listRef).columnGap) || 0;
    const widths = [...measureRef.children].map((el) => (el as HTMLElement).offsetWidth);
    const total = widths.reduce((sum, w, i) => sum + w + (i > 0 ? gap : 0), 0);
    if (total <= available) {
      setVisibleCount(items.length);
      return;
    }
    let used = 0;
    let count = 0;
    for (const w of widths) {
      const next = used + (count > 0 ? gap : 0) + w;
      if (next > available - BURGER_SPACE) break;
      used = next;
      count += 1;
    }
    setVisibleCount(count);
  };

  onMount(() => {
    const observer = new ResizeObserver(recompute);
    observer.observe(listRef!);
    void document.fonts?.ready.then(recompute);
    onCleanup(() => observer.disconnect());
  });

  createEffect(() => {
    if (!open()) return;
    const onDocClick = (e: MouseEvent) => {
      if (overflowRef && !overflowRef.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    onCleanup(() => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    });
  });

  const visible = () => items.slice(0, visibleCount());
  const overflow = () => items.slice(visibleCount());

  return (
    <nav>
      <div class="nav-list" ref={listRef}>
        <For each={visible()}>{(item) => <A href={item.path}>{item.label}</A>}</For>
        <Show when={overflow().length > 0}>
          <span class="nav-overflow" ref={overflowRef}>
            <button
              type="button"
              class="nav-burger"
              aria-label="More pages"
              aria-expanded={open()}
              onClick={() => setOpen((v) => !v)}
            >
              <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
                <path
                  d="M2 4h12M2 8h12M2 12h12"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                />
              </svg>
            </button>
            <Show when={open()}>
              <div class="nav-menu">
                <For each={overflow()}>
                  {(item) => (
                    <A href={item.path} onClick={() => setOpen(false)}>
                      {item.label}
                    </A>
                  )}
                </For>
              </div>
            </Show>
          </span>
        </Show>
        <div class="nav-measure" ref={measureRef} aria-hidden="true">
          <For each={items}>{(item) => <span>{item.label}</span>}</For>
        </div>
      </div>
      <button type="button" onClick={() => refetch()}>
        Refresh
      </button>
    </nav>
  );
}

function Shell(props: { children?: JSX.Element }): JSX.Element {
  return (
    <>
      <header>
        <h1>braincrawl</h1>
        <Nav />
      </header>
      <main>{props.children}</main>
    </>
  );
}

// Paths reachable without an authed session: the auth flow itself, and Settings
// (the manual-token escape hatch the auth page links to when locked out).
const UNGUARDED_PATHS = new Set(["/auth", "/settings"]);

function App(): JSX.Element {
  const allRoutes = plugins.flatMap((plugin) => plugin.routes);
  const guardedRoutes = allRoutes
    .filter((route) => !UNGUARDED_PATHS.has(route.path))
    .map((route) => <Route path={route.path} component={route.component} />);
  const unguardedRoutes = allRoutes
    .filter((route) => UNGUARDED_PATHS.has(route.path))
    .map((route) => <Route path={route.path} component={route.component} />);
  return (
    <GraphProvider>
      <HashRouter root={Shell}>
        <Route path="/" component={() => <Navigate href="/chat" />} />
        <Route component={RequireAuth}>{guardedRoutes}</Route>
        {unguardedRoutes}
        <Route path="*" component={() => <p>Not found: {window.location.hash}</p>} />
      </HashRouter>
    </GraphProvider>
  );
}

export default App;
