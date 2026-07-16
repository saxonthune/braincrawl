---
title: Designing tools for LLM agents — the agent-computer interface
summary: Domain-neutral reference on tool design (the agent-computer interface). What makes a single tool good — granularity shaped to the agent's workflow, descriptions written like onboarding docs, schemas that make misuse unrepresentable, token-efficient returns, actionable errors, and empirical evaluation with the model in the loop. Ends with a tool-design checklist.
tags: [reference, tools, aci, agent, schema, evaluation, product]
deps: []
---

# Designing Tools for LLM Agents: The Agent-Computer Interface

When a language model acts as an agent, its tools are the only way it can reach anything
outside its own text: files, databases, services, other systems. The set of tool names,
descriptions, input schemas, and return shapes the model sees is a real interface, and it
rewards design effort the way a human interface does. This document collects the
practitioner and research consensus on what makes a single tool good — legible to the
model, cheap in tokens, hard to misuse, and reliable when measured — and flags where the
sources disagree.

## What a tool and the ACI actually are

A tool is a structured output that the model requests and deterministic code executes. The
model emits JSON that names a tool and fills its arguments; your own non-model code parses
that JSON and decides what actually happens [12factor-04]. The "12-factor agents" write-up
makes the decoupling explicit: "the LLM decides what to do, but your code controls how it's
done" — a tool call is flexible scaffolding, not a rigid function invocation, so the same
requested call need not run the same way every time [12factor-04]. Anthropic frames the same
point as a contract: tools are "a new kind of software which reflects a contract between
deterministic systems and non-deterministic agents." Unlike ordinary code, where
`getWeather("NYC")` always returns the same thing, an agent may call a tool inconsistently,
skip it, hallucinate its use, or misread a parameter — so the design has to absorb that
unpredictability [anthropic-tools].

The **agent-computer interface (ACI)** is the whole surface the model sees: the tool names,
the descriptions, the input schemas, and the shape of what comes back. Anthropic's rule of
thumb is to weigh how much effort goes into a human-computer interface and "plan to invest
just as much effort in creating good agent-computer interfaces" — on one internal SWE-bench
build they spent more time optimizing the tools than the overall prompt [anthropic-agents].
The reason the interface carries so much weight is that the model only knows what the
interface tells it. It has no access to your source, your intentions, or your API's real
docs — only the strings you put in front of it. Getting those strings right is the work.

## Tool granularity and surface design

The strongest single recommendation across the practitioner sources is: **build tools for
the agent's workflow, not as one-to-one wrappers around an existing API** [anthropic-tools].
A tool that mirrors a REST endpoint inherits an interface designed for deterministic callers
with unlimited patience and unlimited memory; an agent has neither. Anthropic's worked
examples all consolidate:

- Instead of separate `list_users`, `list_events`, and `create_event` calls, expose one
  `schedule_event` tool that does the task [anthropic-tools].
- Instead of `read_logs` that dumps everything, expose `search_logs` that returns only the
  matching lines with surrounding context [anthropic-tools].
- Instead of making the agent chain `get_customer_by_id`, `list_transactions`, and
  `list_notes`, expose one `get_customer_context` that compiles the relevant information in
  one call [anthropic-tools].

The motivation is context economy. Agents have a limited context window; a tool that returns
all contacts when the agent needs one "is wasting its limited context space on irrelevant
information" [anthropic-tools]. Good tools let an agent subdivide and solve a task the way a
person would, while spending as little intermediate context as possible [anthropic-tools].

This pushes toward **fewer, higher-level, task-shaped tools**. But the pressure is not all
in one direction, and the sources name both costs:

- **Too many thin tools** inflates the token cost of the tool list itself, and — more
  importantly — raises the rate of selection errors, where the model picks the wrong tool or
  invents one. OpenAI offers a concrete soft ceiling: "aim for fewer than 20 functions
  available at the start of a turn," and evaluate at different counts [openai-fc]. For very
  large tool sets, both Anthropic and OpenAI now recommend a *tool-search* mechanism that
  defers rarely-used tools and loads them on demand rather than holding them all in context
  [openai-fc][anthropic-tooluse].
- **Too few overloaded tools** forces a single tool to accept a sprawling, mode-switched
  schema, which is its own source of misuse and hard to describe cleanly.

The academic work explains *why* selection error scales with surface size. Gorilla (Patil et
al. 2023) found that a base model's central failure modes when connected to many APIs are
generating inaccurate input arguments and "hallucinating the wrong usage of an API call";
their fix was to make the model retriever-aware, pulling the relevant API's documentation
into context at inference time rather than relying on the model to hold every API in its
weights [gorilla]. The design lesson generalizes beyond their fine-tuning method: the more
tools in play, the more the right documentation has to be actively surfaced, not assumed.

**Namespacing** is the standard mitigation once a surface grows. Anthropic recommends prefix-
or suffix-based grouping — `asana_search` / `jira_search` by service, or
`asana_projects_search` / `asana_users_search` by resource — and notes the choice between
prefix and suffix produces measurable performance differences, so it is worth testing both
[anthropic-tools].

## Writing tool descriptions and schemas

Because the model only knows what the interface says, **the description is the tool**. The
consensus technique is to write it the way you would onboard a new colleague. Anthropic:
"Think of how you would describe your tool to a new hire on your team. Consider the context
that you might implicitly bring… and make it explicit" [anthropic-tools]. OpenAI phrases the
bar as the "intern test": if an intern could not use the function correctly from the schema
alone, it needs more detail [openai-fc]. Anthropic's agent-building guidance asks the same
question — "Is it obvious how to use this tool, based on the description and parameters, or
would you need to think carefully about it?" — and recommends including example usage, edge
cases, input-format requirements, and clear boundaries against other tools
[anthropic-agents].

What a good description contains, drawn across the sources:

- **Purpose** — what the tool does, in plain terms.
- **When to use it, and when *not* to** — OpenAI explicitly recommends stating both, and
  putting "when and when not to use each function" in the system prompt [openai-fc].
- **Return shape** — what the output represents, so the model can plan the next step
  [openai-fc].
- **Failure modes and constraints** — specialized query formats, niche terminology, and
  how resources relate, all stated rather than assumed [anthropic-tools].

Descriptions repay disproportionate attention: Anthropic reports that "even small refinements
to tool descriptions can yield dramatic improvements" in accuracy [anthropic-tools].

The **input schema** carries the load the prose cannot. Use it to make wrong calls
structurally hard:

- **Unambiguous parameter names** — `user_id`, not `user` [anthropic-tools]; and clear names
  over abbreviations, `get_weather` over something obscure [openai-fc].
- **Per-parameter descriptions with format examples** — e.g. a location described as "City
  and country e.g. Bogotá, Colombia" rather than a bare "location" [openai-fc].
- **Enums and object structure that make invalid states unrepresentable** — OpenAI's example
  is avoiding `toggle_light(on: bool, off: bool)`, which permits the contradictory
  `on=true, off=true`; an enum forbids it [openai-fc].
- **Required fields and constraints** stated in the schema, not left implicit.

**Strict / structured-output modes** raise schema adherence from "usually" to "guaranteed."
OpenAI's `strict: true` makes calls reliably conform, at the cost of two schema rules: every
object needs `additionalProperties: false`, and every property must be listed as required —
you express genuinely optional fields by adding `null` as an allowed type (`"type":
["string", "null"]`) [openai-fc]. Anthropic exposes the same guarantee: adding `strict: true`
to a custom tool ensures calls "always match your schema exactly" [anthropic-tooluse]. MCP
mirrors this on the output side with an optional `outputSchema`; when present, servers must
return conforming structured results and clients should validate them, which helps the model
parse and use the output correctly [mcp-tools].

There is a genuine **tension over where detail belongs** — in the prose description or in the
schema. OpenAI leans toward encoding constraints structurally (enums, strict mode, invalid
states made unrepresentable) so the model cannot express a bad call at all [openai-fc].
Anthropic leans toward rich natural-language description, treating the prose as the primary
lever and reporting large gains from refining it [anthropic-tools]. In practice these are
complementary — encode what the schema *can* express, describe what it cannot — but when
effort is limited the two traditions would spend it differently.

## Return-value design and token efficiency

Tool responses are usually the largest sink of context an agent has, so response shape is a
first-class design choice, not an afterthought [anthropic-tools]. The techniques:

- **Pagination, range selection, filtering, and truncation, with sensible defaults.**
  Anthropic recommends "a combination of pagination, range selection, filtering, and/or
  truncation with sensible default parameter values"; Claude Code's tool responses default to
  a 25,000-token cap [anthropic-tools]. MCP's `tools/list` is itself paginated via cursors,
  modeling the same discipline at the protocol level [mcp-tools].
- **Steer the agent toward efficient strategies when you truncate.** Rather than silently
  cutting output, the response can "directly encourage agents to pursue more token-efficient
  strategies, like making many small and targeted searches instead of a single, broad search"
  [anthropic-tools].
- **Concise-vs-detailed response modes.** Anthropic suggests an optional `response_format`
  enum (for example `"concise"` vs `"detailed"`) that lets the agent choose verbosity — a
  short natural-language answer when reasoning, full technical detail with IDs when a
  downstream call needs it [anthropic-tools].
- **Return human-readable names, not opaque IDs.** Resolving "arbitrary alphanumeric UUIDs to
  more semantically meaningful and interpretable language… significantly improves Claude's
  precision in retrieval tasks by reducing hallucinations." Prefer fields like `name`,
  `image_url`, and `file_type` over `uuid`, `256px_image_url`, and `mime_type`
  [anthropic-tools].
- **Response format is itself a design variable.** There is no universal best; JSON, XML, and
  Markdown each perform differently by task and by what the model saw in training, so the
  format should be chosen empirically [anthropic-tools]. This echoes the agent-building advice
  to keep formats "close to what the model has seen naturally occurring in text on the
  internet" and to avoid formats that demand precise counting or heavy escaping
  [anthropic-agents].

A related structural note from MCP: a tool may return **resource links** rather than inlining
large content, letting the client fetch the body only if needed — a way to keep a reference in
context without paying for the payload [mcp-tools].

## Error design

Errors are context the model reads and acts on, so they should steer it to the correct next
call. Anthropic's contrast is direct: an opaque error code or a raw traceback is unhelpful; a
good error "clearly communicates specific and actionable improvements," ideally with an example
of correctly formatted input or a statement of the constraint that was violated
[anthropic-tools]. Gorilla's findings give the mechanism teeth — bad argument generation is a
primary failure mode, so an error that names *which* argument was wrong and *what* valid form
looks like closes the loop the model would otherwise thrash in [gorilla]. MCP separates two
error channels for exactly this: protocol errors (unknown tool, invalid arguments) versus
tool-execution errors reported in the result with `isError: true`, so the model can tell "I
called wrong" from "the world pushed back" [mcp-tools]. The framing to hold onto: an error is
not a dead end, it is the input to the model's self-correction.

## Making tools hard to misuse

The design goal is that wrong usage should be structurally difficult — the software equivalent
of a connector that only fits one way. The concrete moves:

- **Make invalid states unrepresentable in the schema** (enums over free booleans; required
  disambiguating fields) so a malformed call cannot be expressed [openai-fc].
- **Reject malformed input early** with an actionable error rather than acting on a bad guess
  [anthropic-tools]. MCP states servers "MUST validate all tool inputs" and sanitize outputs
  [mcp-tools].
- **Require disambiguation the model tends to skip.** For example, requiring an absolute path
  removes the ambiguity of a relative one. The general rule: if a parameter has a dangerous
  default interpretation, force the caller to state it.
- **Guard against under-specified calls.** Anthropic's docs note that when a required
  parameter is missing from the user's request, more capable models tend to ask for it, but
  less capable ones "might infer a reasonable value" and guess — so a tool that must not be
  guessed at should make the parameter required and its absence a hard, explained failure
  [anthropic-tooluse].

For **mutating (write) tools**, the extra considerations are confirmation, idempotency, and
reversibility. MCP's guidance is explicit that there "SHOULD always be a human in the loop
with the ability to deny tool invocations," that clients should show tool inputs before
execution to prevent accidental or malicious data exfiltration, and should prompt for
confirmation on sensitive operations [mcp-tools]. MCP also carries optional tool *annotations*
that describe behavior such as read-only versus destructive — with the caution that clients
must treat annotations from untrusted servers as untrusted [mcp-tools]. Idempotency (a
repeated call does not compound its effect) matters more for agents than for ordinary
software precisely because agents retry, duplicate, and misfire.

## Evaluating tools empirically

None of the above is settled by taste; the sources insist tools be measured with the model in
the loop. Anthropic's recommended metrics [anthropic-tools]:

- Top-level task accuracy (did the whole task succeed).
- Total tool calls, and runtime per call and per task (redundancy and cost signals).
- Total token consumption.
- Tool-call error counts and error rates.
- Which tools the agent actually reaches for (revealing workflow and mis-selection).

The evaluation method is an **agentic loop**: a simple `while`-loop alternating an LLM call and
a tool call, one loop per evaluation task [anthropic-tools]. Tasks should be realistic and
require several chained calls — "Customer ID 9182 reported they were charged three times. Find
all relevant log entries and determine if other customers were affected" is a good task; a
single-hop "schedule a meeting with jane@acme.corp next week" is a weak one [anthropic-tools].
Prompting the eval agent to emit reasoning/feedback blocks before each call both improves its
performance and exposes where it gets confused; Anthropic advises reading the raw transcripts,
since "what agents omit in their feedback… can often be more important than what they include"
[anthropic-tools]. Then iterate: refine descriptions and schemas against the measured outcomes,
because small description changes move the numbers a lot [anthropic-tools]. On the research
side, Gorilla contributes an evaluation idea worth borrowing — an AST-based check that matches
the *structure* of a generated call against valid API signatures to measure hallucination
robustly, rather than string-matching [gorilla].

## Read/retrieval versus write tools

**Retrieval tools** have a specific double job: make the *right* data reachable *and* legible,
at minimal context cost. This is where several threads converge — filter and rank at the tool
boundary rather than dumping (`search_logs`, not `read_logs`); return semantic identifiers so
the model can refer to results without hallucinating; page and cap the output. The deeper
reason to invest here is grounding: Toolformer (Schick et al. 2023) showed models can learn to
call external tools — a calculator, a Q&A system, search engines — to decide themselves when a
call helps and how to fold the result into what comes next, improving zero-shot performance
without hurting core language ability [toolformer]. A retrieval tool that surfaces real facts
lets the model answer from what it just fetched rather than from unreliable parametric recall;
if the tool returns noise or opaque handles, that grounding is lost. So a retrieval tool's
quality is measured by whether the model ends up citing what it retrieved.

**Write/mutating tools** invert the emphasis: correctness and safety over recall. The moves
from the misuse section apply in full — confirmation and a human able to deny the call,
idempotency so retries are safe, reversibility or at least clear reporting of what changed, and
inputs shown before execution [mcp-tools]. A useful way to hold the split: a read tool's risk
is spending context to little effect; a write tool's risk is doing the wrong thing
irreversibly, and the design should reflect which risk dominates.

## A tool-design checklist

Observed conventions from the sources, framed as checkable rules for a single tool. These are
what the cited practitioners and papers actually recommend, not invented doctrine.

**Naming**
- Give the tool a clear, self-evident name; avoid abbreviations and obscure terms [openai-fc].
- Namespace it (prefix or suffix) once the surface is large; test which scheme reads better
  [anthropic-tools].

**Granularity**
- Shape the tool around the agent's task, not a one-to-one wrapper of an underlying API
  endpoint [anthropic-tools].
- Consolidate a multi-step workflow into one high-level tool where that saves context; split
  only when a single schema would become mode-switched and hard to describe [anthropic-tools].
- Keep the active tool count modest — a soft target of under ~20 per turn — and defer the rest
  behind tool search [openai-fc].

**Description**
- Write it like onboarding docs for a new hire; make implicit context explicit
  [anthropic-tools].
- State purpose, when to use, when *not* to, the return shape, and known failure modes
  [openai-fc][anthropic-agents].
- Pass the "intern test": schema alone should be enough to use it correctly [openai-fc].

**Schema**
- Name parameters unambiguously (`user_id`, not `user`) and describe each with a format example
  [anthropic-tools][openai-fc].
- Use enums and object structure so invalid states cannot be expressed [openai-fc].
- Mark required fields; consider strict/structured-output mode to guarantee conformance
  [openai-fc][anthropic-tooluse].

**Returns**
- Default to concise; offer a detailed mode via a `response_format` enum [anthropic-tools].
- Paginate, filter, range-select, or truncate with sane defaults and a response-size cap
  [anthropic-tools].
- Return human-readable names, not opaque UUIDs [anthropic-tools].
- Choose the response format (JSON / XML / Markdown) by measurement, not habit
  [anthropic-tools].

**Errors**
- Make each error actionable: name what was wrong and show a correct example; never surface a
  bare code or traceback [anthropic-tools].
- Distinguish "you called wrong" from "the operation failed" [mcp-tools].

**Misuse prevention**
- Reject malformed input early; validate all inputs [anthropic-tools][mcp-tools].
- Force disambiguation the model tends to skip (e.g. absolute over relative paths); make a
  must-not-guess parameter required [anthropic-tooluse].
- For writes: confirmation / human-deny, idempotency, reversibility, and show inputs before
  executing [mcp-tools].

**Evaluation**
- Measure with the model in an agentic loop: task success, tool-call count, tokens, error rate,
  and which tools get chosen [anthropic-tools].
- Use realistic multi-step eval tasks; read raw transcripts; iterate descriptions and schemas
  against the numbers [anthropic-tools].

## Where the sources disagree

- **Consolidate vs split.** Anthropic pushes hard toward few high-level, task-shaped tools for
  context economy [anthropic-tools]; OpenAI accepts more granularity but caps it (~20/turn) and
  leans on tool search past that [openai-fc]. The reconciling variable is context cost per
  call versus selection error per tool — which dominates depends on your surface size.
- **Description prose vs schema structure.** Anthropic treats the natural-language description
  as the primary lever and reports large gains from refining it [anthropic-tools]; OpenAI
  treats structural constraints (enums, strict mode, unrepresentable invalid states) as the
  first defense [openai-fc]. Both work; under a limited effort budget they would spend it in
  different places.
- **Response format.** The sources agree there is *no* universal best format and that it must
  be chosen empirically per task and model [anthropic-tools] — itself a disagreement with any
  fixed "always return JSON" convention.

## Sources

- [anthropic-tools] Anthropic Engineering, "Writing effective tools for AI agents."
  https://www.anthropic.com/engineering/writing-tools-for-agents — Primary practitioner
  source; the most detailed single treatment of granularity, descriptions, return-value/token
  design, and empirical evaluation.
- [anthropic-agents] Anthropic Engineering, "Building effective agents" (ACI section).
  https://www.anthropic.com/engineering/building-effective-agents — Practitioner; the
  HCI-parity framing and format-choice guidance.
- [anthropic-tooluse] Anthropic / Claude docs, "Tool use with Claude" (overview).
  https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview — Official docs;
  schema/`input_schema` mechanics, strict tool use, missing-parameter behavior, tool search.
- [mcp-tools] Model Context Protocol, "Tools."
  https://modelcontextprotocol.io/docs/concepts/tools — Protocol spec; tool structure
  (name/description/inputSchema/outputSchema/annotations), error channels, human-in-the-loop
  and validation conventions.
- [openai-fc] OpenAI Platform docs, "Function calling."
  https://developers.openai.com/api/docs/guides/function-calling — Official docs; naming,
  descriptions, strict mode, enums/unrepresentable invalid states, the ~20-function guideline,
  the "intern test."
- [12factor-04] HumanLayer, "12-Factor Agents — Factor 4: Tools are just structured outputs."
  https://github.com/humanlayer/12-factor-agents/blob/main/content/factor-04-tools-are-structured-outputs.md
  — High-quality practitioner write-up; the decoupling of model decision from deterministic
  execution.
- [toolformer] Schick et al., "Toolformer: Language Models Can Teach Themselves to Use Tools"
  (2023). https://arxiv.org/abs/2302.04761 — Foundational academic; models learning when/how to
  call tools and fold results back in, grounding vs parametric recall.
- [gorilla] Patil et al., "Gorilla: Large Language Model Connected with Massive APIs" (2023).
  https://arxiv.org/abs/2305.15334 — Academic benchmark; argument-generation and wrong-usage
  hallucination as primary failure modes, retriever-awareness for large tool sets, AST-based
  hallucination metric.
