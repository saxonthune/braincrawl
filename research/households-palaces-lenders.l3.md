---
domain: mesopotamia-households-palaces-lenders
braincrawl_server: http://127.0.0.1:8787
updated: 2026-06-17
note: >
  Demonstration L3 artifact produced by dogfooding the `braincrawl` skill.
  In real use this file lives in the *consumer's* repo (the game), not in braincrawl.
  Reference-never-copy: ids are the source of truth; titles/notes are convenience only.
---

# L3 — Households, palaces, and private lenders as economic units

Frames the loop-vs-accumulator model: was the economy a few big institutional
households (palace + temple) that redistribute, or a continuum from family
households up through "great organizations," with private credit threading between?

## Questions (aim-directed frontier)
- Q1  How did the household (é / bītum) function as the base economic unit?
- Q2  Were palaces/temples just *big households*, or a categorically different
      (redistributive/market) institution?
- Q3  Private lenders vs the institutional economy — competition, symbiosis, or
      parasitism? Where does private credit sit relative to the palace household?

## Selection set  (canonical id → tags · note)
- openalex:W2165758805  #landmark #Q1 #Q2  Wengrow/“Households & the Emergence of Cities” 2014, cited 150× —
    cities as a *metaphorical extension of the household*; urbanism = scaled-up oikos. Direct yes-ish to Q2.
- openalex:W94828380    #landmark #Q1 #hub  “Institutional, Communal, and Individual Ownership… Arable Land” 1995, cited 127× —
    the land-tenure spectrum (institutional ↔ communal ↔ individual). The forward-snowball hub for this domain.
- openalex:W1974601532  #primary #Q3  “Shepherds, Merchants, and Credit: Lending Practices in Ur III” 2004, cited 47× —
    KEY for Q3: “despite the overwhelming scale of the institutional economies, there was significant room for
    non-institutional households to pursue economic gains through money-lending.” Symbiosis, not just parasitism.
- openalex:W2108033681  #review #Q2 #Q3  “Factor Markets and Ancient Middle Eastern Economies: A Survey” 2014 —
    land/labour/capital markets *did* exist in Iraq (~2000 BCE, long 6th c., 8–9th c. CE). Pushes back on pure-redistribution.
- openalex:W99391817    #review #Q2  Silver, “Redistribution and markets… updating Polanyi” —
    the Polanyi redistribution-vs-market debate itself. WEAK HUB (no abstract, 0 refs, forward-cites drift to maritime trade).
- openalex:W1907933027  #context #Q1  “Family Archives in Mesopotamia (Old Babylonian)” 2013 —
    methodology: how the private household’s records (the é archive) are structured; the documentary base for Q1/Q3.
- openalex:W356093532   #context #Q1  “Slaves and households in the Near East” — household labour composition.
- openalex:W2007836667  #open #Q3   “Structure, Agency and Commerce in the Ancient Near East” — not yet read.
- openalex:W2323559057  #comparative #Q2  “Redistribution in Aegean Palatial Societies” — outside-domain control on the palace-redistribution model.
- openalex:C85064482    #concept  OpenAlex “Mesopotamia” concept — the drift-control filter for forward expansion.

## Domain edges  (src --type--> dst)
- openalex:W2165758805 --supports--> Q2      # palace/city is household writ large
- openalex:W2108033681 --refutes--> "pure redistribution model"   # markets existed
- openalex:W1974601532 --supports--> Q3      # private credit coexists with institutional economy
- openalex:W2323559057 --tensions-with--> openalex:W2165758805    # Aegean redistribution vs Meso household-extension
- openalex:W99391817   --builds-on--> "Polanyi"                   # and is the node others cite for the debate

## Findings / annotations
- [Q2] Strongest in-domain answer so far: **yes, with a caveat.** The household was the
  organizing *metaphor*; palace and temple were the largest instances of one continuum, not
  a separate ontology (W2165758805). But "big household" ≠ "purely redistributive" —
  factor markets and private credit operated alongside (W2108033681, W1974601532).
- [Q3] Ur III shows private money-lending thriving in the *gaps* of a dominant institutional
  economy (W1974601532) — supports a loop/accumulator reading: institutional redistribution
  is the cyclic baseline; private debt is the one-way accumulator that builds up between resets.
- [open] Need a true Assyriological synthesis on temple-vs-palace-vs-private (Renger, Van De
  Mieroop, Steinkeller). OpenAlex coverage of core Assyriology is patchy — many landmark
  works have no abstract and no referenced_works (MAG-era records).

## Coverage / method notes (dogfooding)
- Seed → forward-snowball (cited-by) → neighborhood. Backward refs were empty for the old
  MAG records (W99391817, W94828380 returned 0 refs).
- Citation drift is severe forward-mining by raw influence: W94828380's 127 cited-by include
  a whole "Food as Commons" cluster, Cambridge Greco-Roman volume chapters, an Indian Ocean
  volume, and beer-history front-matter. **Drift control via `concepts.id:C85064482` cut
  127 → 14** and kept the household works. Use the filter, then rank within the subgraph.
- Single-hub snowball yields a star graph, so in-degree ranking is not yet meaningful —
  needs a second overlapping snowball (e.g. cited-by W2165758805) before neighborhood ranking earns its keep.
