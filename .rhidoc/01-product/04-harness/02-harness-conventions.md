---
title: Agent harness — output and interaction conventions
summary: Domain-neutral reference on agent-harness design — the loop around an LLM that runs the reason/act/observe cycle and decides how work is shown. Collects output and interaction conventions from primary sources — the narration/action mix, a default output contract, streaming and status, error recovery, and context management.
tags: [reference, agent, harness, output-conventions, streaming, product]
deps: []
---

# Agent-Harness Design: Output and Interaction Conventions

An **agent harness** is the program loop wrapped around a large language model (LLM)
that turns a text-completion engine into a working assistant: it runs the tool-call
cycle, carries the conversation history, streams output, marks turn boundaries, and
decides how the model's work is shown to a person. This document collects how
well-regarded harnesses behave — with a focus on the *output and interaction*
conventions that make a harness feel responsive and trustworthy rather than noisy.
Every substantive claim is traceable to a cited source listed at the end. It is
domain-neutral background: general harness knowledge, not braincrawl-specific intent.

## What an agent harness is (and is not)

A useful separation runs through four parts:

- **The model** predicts the next tokens. On its own it neither calls tools nor keeps
  state between requests.
- **The tools** are functions the model can ask to run. Anthropic frames them plainly:
  "Tools are just structured outputs" — the model emits a structured request, and
  deterministic code executes it and returns a result [12factor]. Tool design is its
  own discipline: Anthropic treats the agent-computer interface with the same rigor as
  a human-computer interface, and reports it spent more effort optimizing tools than
  the overall prompt on its coding agent [anthropic-agents].
- **The user interface (UI)** is what the person sees and types into.
- **The harness** is the loop that binds these: it takes the user's message, calls the
  model, executes any tool the model requests, feeds the result back, repeats until the
  model stops, and manages what all of this looks like along the way.

Anthropic draws a further line inside this space between *workflows* — "systems where
LLMs and tools are orchestrated through predefined code paths" — and *agents* — "systems
where LLMs dynamically direct their own processes and tool usage" [anthropic-agents]. A
harness for an agent must cope with an unknown number of steps, which raises the stakes
on everything below: cost, error compounding, and how progress is surfaced.

## The loop: reason, act, observe

The core cycle is the ReAct pattern (Yao et al. 2022): the model interleaves *reasoning
traces* with *actions*, so that "reasoning traces help the model induce, track, and
update action plans as well as handle exceptions, while actions allow it to interface
with external sources … to gather additional information" [react]. Reasoning alone
hallucinates and propagates errors; acting alone lacks planning. Interleaving grounds
each step in real feedback from the environment.

Anthropic states the operational form of this directly: "during execution, it's crucial
for the agents to gain 'ground truth' from the environment at each step … to assess its
progress" [anthropic-agents]. The harness runs this as a loop:

1. Send history + tools to the model.
2. The model responds with text, tool calls, or both.
3. If it requested tools, execute them and append the results to history.
4. Repeat until the model returns a final answer with no tool calls, or a stop
   condition trips.

**Stop conditions and caps.** Because an agent's step count is not known in advance,
harnesses add guardrails: an iteration cap, a token/cost budget, and human checkpoints.
Anthropic recommends checkpoint pauses "for human feedback … when encountering blockers"
and warns agents carry "higher costs, and the potential for compounding errors,"
requiring guardrails and sandboxed testing [anthropic-agents]. An unbounded loop is a
known failure mode, not a feature.

**Own the control flow.** Practitioner guidance ("12-factor agents") argues against
handing the loop entirely to a framework: "Own your control flow," "Own your prompts,"
and "Own your context window" — implement explicit routing and state rather than
trusting an opaque agentic loop [12factor].

## The narration/action mix — when to speak, when to act

This is the heart of the interaction design: how a harness interleaves plain-language
explanation with tool calls. The Claude Code prompt encodes a specific, teachable
default [dbreunig, cc-search]:

- **Assume the work is invisible.** Write "for a person, not logging to a console," and
  assume "users can't see most tool calls" [dbreunig]. The narration exists to keep the
  person oriented, not to echo machine activity.
- **A brief preamble before acting.** Anthropic's broader agent guidance favors
  transparency by "explicitly showing the agent's planning steps" [anthropic-agents]. In
  practice a good harness says one short line about what it is about to do before a
  batch of tool calls — enough to set expectations, no more.
- **Terse progress between actions.** The internal Claude Code limit is stark: "keep
  text between tool calls to ≤25 words" [dbreunig]. Progress narration is a thin thread,
  not a paragraph per step.
- **A terse final summary.** The final response limit is "≤100 words unless the task
  requires more detail" [dbreunig]. External guidance is qualitative but aligned: "Go
  straight to the point. Try the simplest approach first without going in circles. Do
  not overdo it. Be extra concise" [dbreunig, cc-search].
- **Skip preamble and postamble.** No "Great question!", no restating the task, no
  "Let me know if you need anything else." Answer, then stop [dbreunig].

The balance is: speak *little* and *at the boundaries* — a short intent line before a
cluster of actions, minimal threading between them, and a compact summary at the end.
The actions themselves are the work; narration is orientation, priced in words.

The exact figures above (≤25 words between tool calls, ≤100-word finals) trace to a
practitioner reverse-engineering of the Claude Code prompt, not an official Anthropic
document, so treat the numbers as reported rather than canonical — though two independent
secondary sources agree on them [dbreunig, cc-search].

## Output formatting conventions

- **Terse by default.** Response length scales to the task; short questions get short
  (even one-word) answers, and the model should not pad [dbreunig]. Brevity is enforced
  at a higher priority than user configuration files in Claude Code's prompt stack
  [cc-search].
- **Markdown, rendered in monospace.** Output is GitHub-flavored markdown "rendered in a
  monospace font" following CommonMark [dbreunig, cc-search]. This shapes choices:
  headings and fenced code blocks read well; heavy tables and nested formatting often do
  not, in a terminal.
- **Lists vs prose.** Prefer prose for explanation and reserve lists for genuinely
  enumerable items. Over-listing is a form of padding.
- **File and line references.** Cite code locations with a path, and prefer a
  `file.ext:line` form so the reference is clickable/navigable [dbreunig, cc-search].
- **Meaningful fields over raw identifiers.** When reporting results the harness (and
  its tools) should return "natural language names, terms, or identifiers" rather than
  cryptic UUIDs, because human-readable fields "directly inform agents' downstream
  actions" — and, equally, a person's understanding [anthropic-tools].

What makes output feel *trustworthy*: it matches the actual work (no invented
confidence), cites where claims come from, and does not bury the answer under
scaffolding. What makes it feel *noisy*: preamble, restated context, per-step logging,
and premature verbosity before the work is done.

## Streaming and status feedback

Streaming matters because an agent turn can be long; without incremental output the user
stares at a blank screen and cannot tell working from hung. The Vercel AI SDK models
this cleanly and is worth adopting as a reference design.

**Message parts, not chat roles.** Rather than one opaque string per message, a message
carries an array of typed `parts`: text segments are `{ type: 'text', text }`, while
tool interactions are their own typed parts (tool call, tool result) rather than
separate chat roles [vercel-ui]. The SDK explicitly recommends rendering "using the
`parts` property instead of the `content` property" [vercel-ui]. This separates the
*wire format* (what the model emits and consumes) from the *display model* (what the
user sees): a tool call becomes a rendered card, a text part becomes prose, all inside
one assistant turn.

**A status field drives the UI.** The SDK exposes four lifecycle states —
`submitted` (sent, awaiting stream start), `streaming` (chunks arriving), `ready`
(complete, ready for the next message), and `error` [vercel-ui]. A "thinking" indicator,
a spinner on a tool-call card, and an input lock all derive from this one field rather
than from ad-hoc flags.

The practical display model: stream text tokens as they arrive; render each tool call as
a compact card that shows the tool name and moves from pending to done; keep the raw
tool result collapsible rather than dumped inline.

## Verification, error handling, recovery

- **Errors are context, not crashes.** "Compact errors into context window": when a tool
  fails, feed a compact version of the error back to the model so it can self-correct,
  instead of breaking the loop [12factor]. Anthropic's tool guidance reinforces this from
  the tool side — error messages should communicate "specific and actionable
  improvements, rather than opaque error codes or tracebacks," steering the model toward
  the right next call [anthropic-tools].
- **Ground truth enables self-correction.** Because the loop observes real results each
  step, the model can notice a wrong turn and adjust — the ReAct reasoning trace is where
  it "handles exceptions" [react, anthropic-agents].
- **Verbal self-reflection (Reflexion).** For retryable tasks, Reflexion (Shinn et al.
  2023) has the agent write a short natural-language critique after a failed attempt and
  prepend it to the next attempt's context — learning "entirely mediated by the context,"
  no weight updates [reflexion]. This is the lineage behind "try, observe the failure,
  reflect, retry" without derailing the surface conversation.

The interaction rule that follows: recover quietly. A failed-then-retried tool call
should not produce a wall of apology and explanation; it is normal loop behavior, shown
as a tool card that failed and one that succeeded, with at most a brief note if the
outcome changed.

## Context management

- **Smallest high-signal set.** The guiding principle is to "find the smallest set of
  high-signal tokens that maximize the likelihood of your desired outcome"
  [anthropic-context].
- **Truncate tool results at the source.** Tool responses are the biggest context sink.
  Anthropic recommends "pagination, range selection, filtering, and/or truncation with
  sensible default parameter values"; Claude Code caps tool responses at ~25,000 tokens
  by default [anthropic-tools]. A configurable `concise` vs `detailed` response format
  lets the agent pull full metadata only when a task needs it, cutting typical usage by
  roughly two-thirds in Anthropic's examples [anthropic-tools].
- **Compaction.** When history approaches the context limit, summarize it and reinitialize
  with the condensed version — but "overly aggressive compaction can result in the loss of
  subtle but critical context whose importance only becomes apparent later," so favor
  recall first and tighten precision later [anthropic-context]. A cheap early win is
  clearing raw tool outputs once they are deep in history and no longer needed
  [anthropic-context].
- **Note-taking outside the window.** Persist progress to structured notes (a to-do or
  progress file) so the agent can "track progress across complex tasks, maintaining
  critical context and dependencies that would otherwise be lost across dozens of tool
  calls" and recover after a context reset [anthropic-context].

## Known anti-patterns

- **Over-narration.** A paragraph per tool call; logging machine activity to a human.
  Countered by the ≤25-words-between-calls discipline [dbreunig].
- **Hidden work.** The opposite failure: a long silent turn with no streaming or status,
  where the user cannot tell progress from a hang. Countered by streaming + a status
  field [vercel-ui].
- **Premature verbosity.** Long explanations before the work is finished; padding the
  answer with preamble/postamble [dbreunig].
- **Silent truncation.** Dropping context or tool output without any signal, so the model
  (or user) acts on a partial picture — the risk Anthropic flags for aggressive
  compaction [anthropic-context].
- **Unbounded loops.** No iteration cap, budget, or stop condition; compounding errors
  with no guardrail [anthropic-agents].
- **Sycophancy / filler.** "Great question!", flattery, and hedging that carries no
  information; the "write for a person, be concise" rules exist partly to suppress this
  [dbreunig].

## A default output contract

Observed conventions, phrased as checkable rules a harness could adopt. These are drawn
from the sources above, not invented.

- **Preamble:** at most one short sentence of intent before a batch of tool calls; skip
  it entirely for a single trivial call. Never restate the user's request [dbreunig,
  anthropic-agents].
- **Progress narration:** keep text between tool calls very short (Claude Code's internal
  target is ≤25 words). It exists to orient, not to log [dbreunig].
- **Final summary:** compact by default (Claude Code's internal target is ≤100 words
  unless the task genuinely needs more). Lead with the answer; no closing pleasantries
  [dbreunig].
- **No emojis** unless the user asks [dbreunig, cc-search].
- **Formatting:** GitHub-flavored markdown; prose over lists unless items truly
  enumerate; `path:line` for code references [dbreunig, cc-search].
- **Stream always** for any turn that can take more than a moment; expose a status field
  (submitted / streaming / ready / error) and drive the "thinking" indicator and input
  lock from it [vercel-ui].
- **Show tool activity as typed parts**, not chat messages: a compact card per tool call
  (name, pending→done), raw result collapsed by default [vercel-ui].
- **On tool error:** feed a compact, actionable error back into context and let the model
  retry; show it as a failed card, not an apology paragraph [12factor, anthropic-tools].
- **Truncate tool results** with sane defaults (pagination/filtering; a hard token cap);
  prefer human-readable fields over raw IDs [anthropic-tools].
- **Bound the loop:** iteration cap, cost budget, and human-checkpoint on blockers
  [anthropic-agents].

## Where sources disagree

- **Single-agent vs. multi-agent.** Cognition argues *against* multi-agent
  architectures: parallel subagents work on conflicting unstated assumptions and produce
  inconsistent results, so prefer a single-threaded linear agent where "context flows
  continuously" — the two principles are "Share context … not just individual messages"
  and "Actions carry implicit decisions, and conflicting decisions carry bad results"
  [cognition]. Anthropic, by contrast, lists multi-agent architectures as a legitimate
  long-horizon strategy where specialized sub-agents "handle focused tasks with clean
  context windows" [anthropic-context]. The tension is real: multi-agent buys context
  isolation at the cost of coordination/consistency. The safe reading for a single
  chat-agent harness is Cognition's — keep one continuous thread unless a task clearly
  decomposes into independent, low-coupling subtasks.
- **How much to narrate.** Anthropic's agent post pushes *transparency* — "explicitly
  showing the agent's planning steps" [anthropic-agents] — while the Claude Code output
  rules push hard for *brevity* [dbreunig]. These are reconcilable (show intent briefly,
  not verbosely), but a harness must pick where on that spectrum it sits; the two
  sources pull in opposite directions at the margin.
- **Framework ownership.** "12-factor agents" argues you should own the loop, prompts,
  and context rather than defer to a framework [12factor]; higher-level SDKs (Vercel AI
  SDK) offer more of the loop and UI out of the box [vercel-ui]. This is a
  control-vs-convenience tradeoff, not a factual conflict.

## Sources

- [anthropic-agents] Anthropic, "Building effective agents" — primary engineering
  guidance; agent-vs-workflow, the ground-truth loop, tool/ACI design, guardrails.
  https://www.anthropic.com/engineering/building-effective-agents
- [anthropic-context] Anthropic, "Effective context engineering for AI agents" —
  compaction, note-taking, smallest-high-signal-tokens, tool-result clearing.
  https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- [anthropic-tools] Anthropic, "Writing effective tools for AI agents" — tool-response
  token efficiency, concise/detailed formats, human-readable fields, actionable errors.
  https://www.anthropic.com/engineering/writing-tools-for-agents
- [react] Yao et al. 2022, "ReAct: Synergizing Reasoning and Acting in Language Models" —
  foundational reason/act/observe interleaving. https://arxiv.org/abs/2210.03629
- [reflexion] Shinn et al. 2023, "Reflexion: Language Agents with Verbal Reinforcement
  Learning" (NeurIPS 2023) — verbal self-critique and retry, context-mediated recovery.
  https://arxiv.org/abs/2303.11366
- [vercel-ui] Vercel AI SDK, `useChat` / chatbot docs — message `parts`, typed tool
  parts, streaming, the `status` lifecycle field. https://ai-sdk.dev/docs/ai-sdk-ui/chatbot
- [12factor] Dexter Horthy / HumanLayer, "12-factor agents" — own prompts/context/control
  flow, tools as structured outputs, compact errors into context, small focused agents.
  https://github.com/humanlayer/12-factor-agents
- [cognition] Cognition, "Don't build multi-agents" — single-threaded context
  engineering; the two context-sharing principles. https://cognition.com/blog/dont-build-multi-agents
- [dbreunig] Drew Breunig, "How Claude Code builds a system prompt" (2026) —
  practitioner analysis quoting Claude Code's tone/style rules (≤25 words between tool
  calls, ≤100-word finals, no preamble/postamble, write for a person).
  https://www.dbreunig.com/2026/04/04/how-claude-code-builds-a-system-prompt.html
- [cc-search] Claude Code system-prompt corpus (Piebald-AI mirror) and Anthropic prompt
  docs, via search — corroborates the concise/no-emoji/markdown/file-reference rules.
  https://github.com/Piebald-AI/claude-code-system-prompts
