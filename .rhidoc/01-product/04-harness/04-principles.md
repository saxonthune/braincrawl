---
title: Practical principles for agent and tool design
summary: A neutral, portable distillation of the harness and tool-design research into practical principles a working session can borrow. Covers the system view, the loop, tool design, output and interaction, context, and measurement. States each principle to be applied; the backing references carry the evidence and citations.
tags: [reference, principles, agent, tools, output-conventions, product]
deps: [doc01.04.02, doc01.04.03]
---

# Practical Principles for Agent and Tool Design

A working distillation of the harness-conventions and tool-design references into
principles a session can apply directly. It is deliberately neutral: no assumption about
any particular harness, project, or domain. Each line is a principle to act on, with its
reasoning compressed; the backing references carry the evidence, the quotations, and the
open disagreements.

## The system

- The model is the one non-deterministic part; everything around it — the loop, the
  tools, the context, the display — is deterministic scaffolding you control. Design to
  *contain* the non-determinism, not to wish it away.
- Keep three things decoupled so each can change on its own: the **model** (swappable),
  the **control** (loop and state, testable with a scripted stand-in model), and the
  **policy** (prompts and tool descriptions, tunable without redeploying code). Behavior
  smeared across all three is what makes a system feel unpredictable.

## The loop

- One turn is reason → act → observe: send history and tools to the model, run whatever
  it requests, append the results, repeat until it answers with no tool call or a stop
  condition trips.
- Bound every loop: an iteration cap, a cost budget, and a human checkpoint on blockers.
  An unbounded loop is a failure mode, not a feature.
- Get ground truth from the environment at each step, so the model can notice a wrong
  turn and correct it rather than compounding the error.

## Tools

- Treat the tool interface as a product. The model knows only what the interface says —
  the names, descriptions, schemas, and return shapes are the entire surface.
- Shape a tool around the agent's task, not as a one-to-one wrapper of an underlying API.
  Consolidate a multi-step chain into one high-level tool where that saves context; split
  only when a single schema would turn mode-switched and hard to describe.
- Keep the active tool surface small; defer rarely-used tools rather than holding them all
  in context. A larger surface raises wrong-tool selection, not just token cost.
- Write descriptions like onboarding docs for a new hire: purpose, when to use, when *not*
  to, the return shape, and known failure modes. Small description refinements move
  accuracy a lot.
- Make invalid calls unrepresentable in the schema: precise parameter names, per-parameter
  format examples, enums instead of free booleans, required fields, and strict/structured
  output where conformance must be guaranteed.
- Design returns for token economy — they are usually the largest context sink. Paginate,
  filter, and truncate with sane defaults and a size cap; offer a concise-vs-detailed
  mode; return human-readable names over opaque identifiers; choose the response format by
  measurement, not habit.
- Make errors actionable: name what was wrong and show a valid example. Distinguish "you
  called it wrong" from "the operation failed" — they lead to different next moves.
- Make misuse structurally hard: reject malformed input early, and force the disambiguation
  the model tends to skip. For tools that change state, require confirmation, make them
  idempotent, and keep them reversible or clearly reported.
- A retrieval tool's job is to make the *right* data reachable *and* legible at minimal
  context cost; its quality is whether the model ends up answering from what it fetched
  rather than from unreliable recall.

## Output and interaction

- Terse by default. Response length scales to the task; a short question gets a short
  answer, and padding is a cost.
- Speak at the boundaries: a short intent line before a batch of actions, minimal
  narration between them, a compact summary at the end. Assume the person cannot see most
  of the tool calls — narration is orientation, not a machine log.
- No preamble, no postamble, no filler. Lead with the answer and stop.
- Stream anything that takes more than a moment, and drive a status indicator from one
  explicit state (waiting, streaming, done, error) rather than ad-hoc flags.
- Show tool activity as its own typed thing — a compact card that moves from pending to
  done — not as a chat message attributed to a speaker. Keep the raw result collapsed.
- Recover quietly. A failed-then-retried action is a failed card and a success card, not a
  paragraph of apology.

## Context

- Aim for the smallest set of high-signal tokens that gets the outcome; irrelevant context
  is not free.
- Truncate tool results at the source (pagination, filtering, caps), not after they have
  already filled the window.
- Compact history carefully when it grows — favor recall over precision, because the
  importance of a dropped detail often shows up later. Clear stale raw tool output that is
  deep in history and no longer needed.
- For long tasks, persist progress to notes outside the context window so it survives a
  reset.

## Measurement

- None of this is settled by taste. Measure with the model in the loop.
- Keep a small set of realistic, multi-step tasks as a fixed yardstick. Track task
  success, tool-call count, token cost, error rate, and which tools the agent reaches for.
- Improve rationally: change one thing, run the set, keep the change if the number moves
  the right way. Read the raw transcripts — what the agent omits is often more telling than
  what it includes.
- Prefer stable building blocks. Give each model call one job and a constrained output,
  ground it in retrieved data rather than recall, and keep its context minimal — remove the
  freedom to vary where you do not want variance, then measure the variance that remains.

## Provenance

Distilled from the output-and-interaction conventions (doc01.04.02) and the tool-design
reference (doc01.04.03), where every claim here is stated at length and tied to its source.
When a claim matters, follow it back rather than trusting this condensation.
