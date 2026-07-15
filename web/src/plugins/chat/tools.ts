import { getSetting } from "./settings";

export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: {
    type: "object";
    properties: Record<string, unknown>;
    required?: string[];
  };
}

export interface ToolResult {
  content: string;
  is_error?: boolean;
}

export type ToolHandler = (input: Record<string, unknown>) => Promise<ToolResult>;

export interface Tool {
  definition: ToolDefinition;
  handler: ToolHandler;
}

const MAX_RESULT_BYTES = 20 * 1024;
const OPENALEX_BASE = "https://api.openalex.org";

function truncate(text: string): string {
  const bytes = new TextEncoder().encode(text);
  if (bytes.length <= MAX_RESULT_BYTES) return text;
  const slice = bytes.slice(0, MAX_RESULT_BYTES);
  const truncated = new TextDecoder().decode(slice);
  return `${truncated}\n…[truncated, ${bytes.length} bytes total]`;
}

function ok(payload: unknown): ToolResult {
  const text = typeof payload === "string" ? payload : JSON.stringify(payload);
  return { content: truncate(text) };
}

function fail(message: string): ToolResult {
  return { content: message, is_error: true };
}

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

async function storeFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(storeUrl(path), { ...init, headers: { ...storeHeaders(), ...(init?.headers as Record<string, string> | undefined) } });
}

// ── openalex_search ─────────────────────────────────────────────────────────

function stripOpenAlexUrl(id: string): string {
  return id.replace(/^https:\/\/openalex\.org\//, "");
}

function stripDoiUrl(doi: string): string {
  return doi.replace(/^https:\/\/doi\.org\//, "");
}

const ENTITY_TO_KIND: Record<string, string | undefined> = {
  works: "Work",
  authors: "Author",
  sources: "Venue",
  topics: "Topic",
  concepts: "Concept",
};

interface Alias {
  namespace: string;
  value: string;
}

function extractAliases(entity: string, record: Record<string, unknown>): Alias[] {
  const aliases: Alias[] = [];
  const id = record.id;
  if (entity === "works") {
    if (typeof id === "string") aliases.push({ namespace: "openalex", value: stripOpenAlexUrl(id) });
    const ids = record.ids as Record<string, unknown> | undefined;
    if (ids) {
      if (typeof ids.doi === "string") aliases.push({ namespace: "doi", value: stripDoiUrl(ids.doi) });
      if (typeof ids.pmid === "string") aliases.push({ namespace: "pmid", value: ids.pmid });
      if (typeof ids.pmcid === "string") aliases.push({ namespace: "pmcid", value: ids.pmcid });
      if (ids.mag !== undefined && ids.mag !== null) {
        aliases.push({ namespace: "mag", value: String(ids.mag) });
      }
    }
  } else if (entity === "authors") {
    if (typeof id === "string") aliases.push({ namespace: "openalex", value: stripOpenAlexUrl(id) });
    const orcid = record.orcid;
    if (typeof orcid === "string") {
      aliases.push({ namespace: "orcid", value: orcid.replace(/^https:\/\/orcid\.org\//, "") });
    }
  } else if (typeof id === "string") {
    aliases.push({ namespace: "openalex", value: stripOpenAlexUrl(id) });
  }
  return aliases;
}

async function pushWork(entity: string, record: Record<string, unknown>): Promise<void> {
  const kind = ENTITY_TO_KIND[entity];
  if (!kind) return;
  const aliases = extractAliases(entity, record);
  if (aliases.length === 0) return;
  const body = { source: "openalex", kind, aliases, attrs: record };
  await storeFetch("/works", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function openalexSearch(input: Record<string, unknown>): Promise<ToolResult> {
  const query = String(input.query ?? "");
  const entity = String(input.entity ?? "works");
  const limit = Number(input.limit ?? 8);
  if (!query) return fail("query is required");

  const params = new URLSearchParams({ search: query, per_page: String(limit), cursor: "*" });
  const res = await fetch(`${OPENALEX_BASE}/${entity}?${params.toString()}`);
  if (!res.ok) return fail(`OpenAlex search failed: ${res.status} ${await res.text()}`);
  const json = (await res.json()) as { results?: Record<string, unknown>[] };
  const results = json.results ?? [];

  if (entity === "works") {
    await Promise.all(results.map((r) => pushWork(entity, r)));
  }

  const summary = results.map((r) => ({
    id: r.id,
    title: r.display_name ?? r.title,
    year: r.publication_year,
    authors: extractAuthorships(r),
  }));
  return ok({ query, entity, count: summary.length, results: summary });
}

function extractAuthorships(record: Record<string, unknown>): string[] {
  const authorships = record.authorships;
  if (!Array.isArray(authorships)) return [];
  return authorships
    .map((a) => {
      const author = (a as Record<string, unknown>).author as Record<string, unknown> | undefined;
      return typeof author?.display_name === "string" ? author.display_name : undefined;
    })
    .filter((v): v is string => !!v);
}

// ── openalex_cited_by ────────────────────────────────────────────────────────

async function openalexCitedBy(input: Record<string, unknown>): Promise<ToolResult> {
  const workId = String(input.work_id ?? "");
  const conceptFilter = input.concept_filter ? String(input.concept_filter) : undefined;
  const limit = Number(input.limit ?? 15);
  if (!workId) return fail("work_id is required");

  let filter = `cites:${workId}`;
  if (conceptFilter) filter += `,concepts.id:${conceptFilter}`;
  const params = new URLSearchParams({ filter, per_page: String(limit), cursor: "*" });
  const res = await fetch(`${OPENALEX_BASE}/works?${params.toString()}`);
  if (!res.ok) return fail(`OpenAlex cited_by failed: ${res.status} ${await res.text()}`);
  const json = (await res.json()) as { results?: Record<string, unknown>[] };
  const results = json.results ?? [];

  await Promise.all(results.map((r) => pushWork("works", r)));

  const fetchedAt = new Date().toISOString();
  const edges = results
    .map((r) => (typeof r.id === "string" ? stripOpenAlexUrl(r.id) : undefined))
    .filter((v): v is string => !!v)
    .map((citingId) => ({
      src: { namespace: "openalex", value: citingId },
      dst: { namespace: "openalex", value: stripOpenAlexUrl(workId) },
      relation: "cites",
      source: "openalex",
      attrs: null,
      fetched_at: fetchedAt,
    }));
  if (edges.length > 0) {
    await storeFetch("/edges", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(edges),
    });
  }

  const summary = results.map((r) => ({
    id: r.id,
    title: r.display_name ?? r.title,
    year: r.publication_year,
    authors: extractAuthorships(r),
  }));
  return ok({ work_id: workId, count: summary.length, results: summary });
}

// ── read_pages ───────────────────────────────────────────────────────────────

interface Chunk {
  text: string;
  page_start: number;
  page_end: number;
}

const chunkCache = new Map<string, Chunk[]>();

async function readPages(input: Record<string, unknown>): Promise<ToolResult> {
  const workId = String(input.work_id ?? "");
  const pageStart = Number(input.page_start);
  const pageEnd = Number(input.page_end);
  if (!workId) return fail("work_id is required");
  if (!Number.isFinite(pageStart) || !Number.isFinite(pageEnd)) {
    return fail("page_start and page_end are required");
  }

  let chunks = chunkCache.get(workId);
  if (!chunks) {
    const res = await storeFetch(`/works/${encodeURIComponent(workId)}/content/chunks`);
    if (res.status === 404) {
      return fail(
        `No page chunks are stored for ${workId} — chunking happens as a desktop step. Tell the user it isn't available yet.`,
      );
    }
    if (res.status === 202) {
      return fail(`Chunks for ${workId} are still being prepared; try again shortly.`);
    }
    if (!res.ok) return fail(`read_pages failed: ${res.status} ${await res.text()}`);
    const json = (await res.json()) as { chunks?: Chunk[] };
    chunks = json.chunks ?? [];
    chunkCache.set(workId, chunks);
  }

  const overlapping = chunks.filter((c) => c.page_end >= pageStart && c.page_start <= pageEnd);
  if (overlapping.length === 0) {
    return ok(`No chunks found for pages ${pageStart}-${pageEnd} of ${workId}.`);
  }
  const text = overlapping
    .map((c) => `[pp.${c.page_start}-${c.page_end}]\n${c.text}`)
    .join("\n\n");
  return ok(text);
}

// ── graph_neighborhood ───────────────────────────────────────────────────────

async function graphNeighborhood(input: Record<string, unknown>): Promise<ToolResult> {
  const seeds = input.seeds;
  const dir = String(input.dir ?? "forward");
  const depth = Number(input.depth ?? 1);
  const maxNodes = Number(input.max_nodes ?? 40);
  if (!Array.isArray(seeds) || seeds.length === 0) return fail("seeds must be a non-empty array");

  const res = await storeFetch("/graph/neighborhood", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ seeds, dir, depth, max_nodes: maxNodes }),
  });
  if (!res.ok) return fail(`graph_neighborhood failed: ${res.status} ${await res.text()}`);
  return ok(await res.json());
}

// ── l3_read_doc / l3_edit_doc ────────────────────────────────────────────────

const docCache = new Map<string, string>();

async function fetchDoc(slug: string): Promise<string | null> {
  const res = await storeFetch(`/api/l3/docs/${encodeURIComponent(slug)}`);
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`l3 doc fetch failed: ${res.status} ${await res.text()}`);
  const text = await res.text();
  docCache.set(slug, text);
  return text;
}

async function l3ReadDoc(input: Record<string, unknown>): Promise<ToolResult> {
  const slug = String(input.slug ?? "");
  if (!slug) return fail("slug is required");
  try {
    const text = docCache.get(slug) ?? (await fetchDoc(slug));
    if (text === null) return fail(`doc not found: ${slug}`);
    return ok(text);
  } catch (err) {
    return fail(String(err));
  }
}

async function l3EditDoc(input: Record<string, unknown>): Promise<ToolResult> {
  const slug = String(input.slug ?? "");
  const mode = String(input.mode ?? "");
  const text = String(input.text ?? "");
  const oldStr = input.old_str !== undefined ? String(input.old_str) : undefined;
  if (!slug) return fail("slug is required");
  if (mode !== "append" && mode !== "str_replace") return fail('mode must be "append" or "str_replace"');

  let current = docCache.get(slug);
  if (current === undefined) {
    try {
      const fetched = await fetchDoc(slug);
      current = fetched ?? "";
    } catch (err) {
      return fail(String(err));
    }
  }

  let updated: string;
  if (mode === "append") {
    updated = current.endsWith("\n") ? `${current}${text}` : `${current}\n${text}`;
  } else {
    if (!oldStr) return fail("old_str is required for str_replace");
    const occurrences = current.split(oldStr).length - 1;
    if (occurrences !== 1) {
      return fail(`old_str must match exactly once in ${slug} (found ${occurrences})`);
    }
    updated = current.replace(oldStr, text);
  }

  const res = await storeFetch(`/api/l3/docs/${encodeURIComponent(slug)}`, {
    method: "PUT",
    headers: { "Content-Type": "text/markdown" },
    body: updated,
  });

  if (res.status === 200 || res.status === 201) {
    const normalized = await res.text();
    docCache.set(slug, normalized);
    return ok(`doc ${slug} updated (${normalized.length} bytes)`);
  }
  if (res.status === 400 || res.status === 409) {
    const json = await res.json();
    return fail(JSON.stringify(json));
  }
  return fail(`l3_edit_doc failed: ${res.status} ${await res.text()}`);
}

// ── tool registry ────────────────────────────────────────────────────────────

export const tools: Tool[] = [
  {
    definition: {
      name: "openalex_search",
      description:
        "Search OpenAlex for works, authors, sources, topics, or concepts. Pushes work results into the store catalog.",
      input_schema: {
        type: "object",
        properties: {
          query: { type: "string", description: "Full-text search query" },
          entity: {
            type: "string",
            description: "OpenAlex entity collection to search",
            default: "works",
            enum: ["works", "authors", "sources", "topics", "concepts"],
          },
          limit: { type: "integer", description: "Max results", default: 8 },
        },
        required: ["query"],
      },
    },
    handler: openalexSearch,
  },
  {
    definition: {
      name: "openalex_cited_by",
      description:
        "Find works that cite the given work id (forward citations). Pushes works and citation edges into the store.",
      input_schema: {
        type: "object",
        properties: {
          work_id: { type: "string", description: "OpenAlex work id (e.g. W2741809807)" },
          concept_filter: { type: "string", description: "Optional OpenAlex concept id to gate results" },
          limit: { type: "integer", description: "Max results", default: 15 },
        },
        required: ["work_id"],
      },
    },
    handler: openalexCitedBy,
  },
  {
    definition: {
      name: "read_pages",
      description: "Read page-anchored text chunks of a stored work between two book pages.",
      input_schema: {
        type: "object",
        properties: {
          work_id: { type: "string" },
          page_start: { type: "integer" },
          page_end: { type: "integer" },
        },
        required: ["work_id", "page_start", "page_end"],
      },
    },
    handler: readPages,
  },
  {
    definition: {
      name: "graph_neighborhood",
      description: "Read the stored citation graph's neighborhood around one or more seed work ids.",
      input_schema: {
        type: "object",
        properties: {
          seeds: { type: "array", items: { type: "string" }, description: "Seed work ids (ns:value form)" },
          dir: { type: "string", enum: ["forward", "backward"], description: "Direction to traverse" },
          depth: { type: "integer", default: 1 },
          max_nodes: { type: "integer", default: 40 },
        },
        required: ["seeds", "dir"],
      },
    },
    handler: graphNeighborhood,
  },
  {
    definition: {
      name: "l3_read_doc",
      description: "Read a research document's raw markdown by slug.",
      input_schema: {
        type: "object",
        properties: { slug: { type: "string" } },
        required: ["slug"],
      },
    },
    handler: l3ReadDoc,
  },
  {
    definition: {
      name: "l3_edit_doc",
      description:
        "Edit a research document: append text, or replace an exact single occurrence of old_str with text.",
      input_schema: {
        type: "object",
        properties: {
          slug: { type: "string" },
          mode: { type: "string", enum: ["append", "str_replace"] },
          text: { type: "string" },
          old_str: { type: "string", description: "Required for str_replace; must match exactly once" },
        },
        required: ["slug", "mode", "text"],
      },
    },
    handler: l3EditDoc,
  },
];

export function findTool(name: string): Tool | undefined {
  return tools.find((t) => t.definition.name === name);
}
