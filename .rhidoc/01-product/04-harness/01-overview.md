---
title: Overview
summary: The AI chat harness is the runtime that hosts the agent — the loop that carries a chat session's history, calls the model, runs braincrawl's tools on the user's behalf, and streams the work back. This section covers what the harness is, its parts, and the conventions it follows.
tags: [product, harness, agent, chat, overview]
deps: [doc01.02]
---

# Overview

The **AI chat harness** is the runtime that hosts the agent (doc01.02). Where the
mental model describes *what the agent is* — a signal converter that builds the Library
and Catalog and translates between the user and the library — the harness is *the
program loop that runs it*: it carries a chat session's history, sends that history and
the available tools to the model, executes whatever tool the model requests against
braincrawl, feeds the result back, and repeats until the model answers.

## Parts

- **The loop.** One turn is a reason/act/observe cycle: call the model, run any tool it
  requests, append the result, repeat until the model returns a final answer or a stop
  condition trips (an iteration cap or a failure).
- **The tools.** The agent's access to braincrawl — the search providers, the store, and
  the Research Documents. Through them the agent both reads and writes on the
  user's behalf.
- **The model transport.** How a session reaches a model — provider selection and the
  server-side proxy that holds the key, so a browser need not.
- **The display model.** How a session is shown to the person: streamed text, tool
  activity on its own track, and a status indicator, kept separate from the wire format
  the model consumes.

## Relation to the rest

The harness hosts the agent (doc01.02). It is distinct from the Web UI (doc01.03), which
is read-only: the harness both reads and writes braincrawl, because acting on the user's
behalf is the point of a session.

How the harness should behave — the narration/action mix, output conventions, streaming
and status, and quiet error recovery — is collected as a domain-neutral reference in the
harness conventions (doc01.04.02).
