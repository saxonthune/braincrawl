---
title: Mental Model
summary: The agent as signal converter — it builds the Library and Catalog, and translates between the user and the library so a human reads only the best works directly.
tags: [product, mental-model, agent, role]
deps: [doc01.01]
---

# Mental Model

The LLM is a signal converter, and it works best as the medium between sources of
information. In a braincrawl session its job is composed of a few general behaviors.

It builds up the Library and Catalog — work it does faster than a human would in a reference
manager.

It also acts as the translator between the user and the library itself. Only the best works
are read directly by a human. As the session builds toward the moments when a work must be
read directly, the user instead asks a question as a prompt; the LLM carries that
question to the artifacts in the library and returns a response drawn from the library.
