# Window Hop agent instructions

## Before changing the repo

Read `CONTEXT.md`, relevant decisions in `docs/adr/`, and the issue or spec for the work. Product behavior is defined in `docs/product-requirements.md`; Windows acceptance scenarios are in `docs/acceptance.md`.

<!-- codebase-memory-mcp:start -->
For structural codebase exploration, use the installed `codebase-memory` skill. Verify index coverage for every source file relied on and read uncovered lines directly.
<!-- codebase-memory-mcp:end -->

## Agent skills

### Issue tracker

Specs and implementation tickets live in GitHub Issues for `ardli-firman/window-hop`. Read `docs/agents/issue-tracker.md` before creating, fetching, or updating issues.

### Triage labels

Use the five default triage roles. Read `docs/agents/triage-labels.md` before assigning or changing a triage state.

### Domain docs

Use a single context: root `CONTEXT.md` and `docs/adr/`. Read `docs/agents/domain.md` before exploring domain behavior or changing domain documentation.

## Rust implementation

Read `docs/agents/rust.md` before adding or changing Rust, platform integration, dependencies, or module layout. Keep lifecycle rules independent of GPUI and Windows APIs. Every process termination must be limited to a verified profile-owned instance.

## Delivery

Read `CONTRIBUTING.md` before committing or opening a PR. Use Conventional Commits and the repository PR template. Use the spec and ticket templates for GitHub issue bodies; mark only checks actually performed and report missing Windows validation explicitly.
