# Case Study — Why braincrawl Exists

This is the real-world motivation behind the architecture in [GOALS.md](./GOALS.md). It is
written from the user's point of view: the actual project, the actual frustration, and what
"good" would look like.

## The project

I am building a **micro grand-strategy game** set in ancient Mesopotamia. Its core thesis is
*loop-vs-accumulator*: which economic and ecological quantities cycle back to a baseline each
period, and which accumulate one-way until the accumulation breaks the cycle (debt until
jubilee, soil salt until abandonment, land concentration until the reset mechanism itself
fails). To model that honestly I need real historical and archaeological grounding —
Assyriology, ancient economic history, irrigation and salinization science, trade and debt
institutions.

So I have an ongoing need to **gather and organize research about Mesopotamia**, over a long
period, against specific and evolving questions.

## The frustration — one-shot deep research

I have been using an AI "deep research" flow: ask a question, it fans out web searches,
fetches sources, adversarially verifies claims, and returns a report.

It does not fit this kind of work, for two connected reasons:

1. **It is decisive when I want inventory.** The flow is built to *verify and kill* claims —
   it runs each claim through an adversarial gate and discards whatever does not survive.
   Whole question-clusters came back with "no survivors" even where real scholarship plainly
   exists, because the gate is strict, not because the field is empty. What I actually want
   first is a *map* of the major works and positions and where they disagree — coverage, not
   a verdict. (This is the literature-review funnel: handbooks → reviews → citation-mining →
   primary sources, with judgement coming last.)

2. **It throws the research away.** Each run pulls abstracts and full articles essentially at
   random, uses them once, and discards them. The next question starts from zero. There is no
   accumulation. I am paying to re-fetch and re-discover the same foundational works every
   time, and nothing compounds.

## What I actually want

I do not want a smarter one-shot. I want an **agent with access to a growing store of
research** — so that knowledge accumulates instead of evaporating.

Concretely, three things, in order:

1. **A persistent store of research, not a random pull.** When the system fetches an abstract
   or a full article, it goes *somewhere* and stays there. The corpus grows. The second
   question benefits from the first question's reading.

2. **More than a bucket of articles — a neutral graph connecting the data.** A pile of PDFs
   is not understanding. I need the *connections*: which work cites which, which is the
   canonical landmark, which are the rebuttals, who argues what. That graph is general — it is
   not specific to my game — so it should be built once and be reusable. (This is exactly the
   citation-network funnel a human researcher builds in their head; here it is made explicit
   and persistent.)

3. **An ongoing, aim-directed research project on top of it.** My game has specific,
   evolving questions (the loop-vs-accumulator inventory). I want a domain project that *sits
   on top of* the shared graph and cached corpus — it tracks my questions, my findings, my
   annotations — and when it needs something it does not yet have, it asks the graph, and
   only then decides whether to go fetch more. My domain work is separate from, but draws on,
   the general knowledge.

## How this maps to the architecture

The three wants map one-to-one onto the three layers in GOALS.md:

| What I want | Layer |
|---|---|
| Research is stored and accumulates, not re-fetched | **Layer 1 — Corpus** (works keyed and persisted; abstract now, full text on demand) |
| A neutral, reusable graph of how works connect | **Layer 2 — Metadata graph** (citation network, built once, shared across all projects) |
| My game's ongoing, aim-directed research project | **Layer 3 — Consumer projection** (my questions, selections, and annotations, referencing Layer 2) |

The separation is the whole point: **general knowledge is built up once and reused; my
domain needs are carved out on top.** The Mesopotamia game is the first consumer (Layer 3),
but the corpus and the graph beneath it (Layers 1–2) are not about Mesopotamia at all — a
future project on a different topic reuses the same machinery and inherits whatever overlaps.

## The shift in posture

The old flow answered *"is this claim true?"* and threw the work away. braincrawl is built to
answer *"what is the shape of this field, and what do we already know about it?"* — and to
keep what it learns. Verification does not disappear; it moves to query time and becomes a
*lens I choose* over an existing map, instead of a gate that destroys material before I ever
see it.

That is the difference between renting an answer and building a library.
