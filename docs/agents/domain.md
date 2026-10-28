# Domain docs

Window Hop uses a single context. The glossary is `CONTEXT.md` at the repo root; architecture decisions live in `docs/adr/`.

## Read before domain work

Read the glossary before exploring, specifying, or implementing domain behavior. Read ADRs relevant to the area being changed. Use the glossary's terms in issues, tests, code-facing explanations, and review findings.

Product requirements belong in `docs/product-requirements.md`; acceptance scenarios belong in `docs/acceptance.md`. `CONTEXT.md` contains domain vocabulary only, without implementation details or feature requirements.

## Update the model

Use `domain-modeling` when resolving a new term, changing a definition, or recording a substantial architecture decision. Add resolved terms as they emerge. Create an ADR only for a consequential trade-off whose reasoning would otherwise be lost; increment the existing ADR numbering.

If a proposal contradicts an ADR, name the conflict and resolve the decision explicitly before treating the proposal as accepted. Keep accepted behavior and a proposal's status distinguishable.

If a referenced domain document is missing, proceed without treating its absence as a blocker. Create documents lazily when there is an actual term or decision to record. Adopt a context map only after a real split into multiple domains warrants it.
