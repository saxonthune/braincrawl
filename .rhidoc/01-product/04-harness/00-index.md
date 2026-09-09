---
title: AI chat harness
summary:
tags: []
deps: []
---

# AI chat harness


| Ref | Item | Kind | Summary | Tags |
|-----|------|------|---------|------|

| doc01.04.01 | Overview | doc | The AI chat harness is the runtime that hosts the agent — the loop that carries a chat session's history, calls the model, runs braincrawl's tools on the user's behalf, and streams the work back. This section covers what the harness is, its parts, and the conventions it follows. | product, harness, agent, chat, overview |
| doc01.04.02 | Agent harness — output and interaction conventions | doc | Domain-neutral reference on agent-harness design — the loop around an LLM that runs the reason/act/observe cycle and decides how work is shown. Collects output and interaction conventions from primary sources — the narration/action mix, a default output contract, streaming and status, error recovery, and context management. | reference, agent, harness, output-conventions, streaming, product |
| doc01.04.03 | Designing tools for LLM agents — the agent-computer interface | doc | Domain-neutral reference on tool design (the agent-computer interface). What makes a single tool good — granularity shaped to the agent's workflow, descriptions written like onboarding docs, schemas that make misuse unrepresentable, token-efficient returns, actionable errors, and empirical evaluation with the model in the loop. Ends with a tool-design checklist. | reference, tools, aci, agent, schema, evaluation, product |
| doc01.04.04 | Practical principles for agent and tool design | doc | A neutral, portable distillation of the harness and tool-design research into practical principles a working session can borrow. Covers the system view, the loop, tool design, output and interaction, context, and measurement. States each principle to be applied; the backing references carry the evidence and citations. | reference, principles, agent, tools, output-conventions, product |

Topics: aci, agent, chat, evaluation, harness, output-conventions, overview, principles, product, reference, schema, streaming, tools
