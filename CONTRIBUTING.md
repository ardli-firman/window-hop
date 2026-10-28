# Contributing to Window Hop

Read `AGENTS.md` for repository instructions. Engineering workflow settings live in `docs/agents/`; Rust implementation and file layout rules live in `docs/agents/rust.md`.

## Issues and pull requests

Use GitHub Issues for specs and implementation tickets. The issue chooser provides both templates. A spec captures the problem, behavior, decisions, tests, and exclusions. A ticket delivers a demonstrable vertical slice, links its parent spec, and declares blockers and acceptance criteria.

Use `.github/pull_request_template.md` for PRs. Link the implementation ticket with `Closes #<ticket>` only when the PR fulfills it; reference the parent spec without closing it. State the resulting behavior and actual validation results, including outstanding Windows testing.

## Conventional Commits

Every new commit subject and PR title uses:

```text
type(scope): description
```

The scope is optional. Repository types are `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, and `revert`. Use a short lowercase scope when useful, such as `profiles`, `browser`, `windows`, `ui`, or `installer`. Write a concise description of the change; prefer imperative English.

Examples:

```text
feat(profiles): activate a profile from its shortcut
fix(windows): stop the owned browser on monitor disconnect
docs: document profile activation behavior
refactor(browser)!: change the isolated launch contract
```

Mark breaking changes with `!` before the colon, a `BREAKING CHANGE:` footer, or both. Use the body for the reason and the footer for issue references. Keep unrelated changes in separate commits. Give merge and revert commits conventional subjects too; edit Git's default message when necessary.

Install the repository's local commit-message hook after cloning:

```sh
git config --local core.hooksPath .githooks
```

This changes only this clone. The hook checks the first non-comment, nonblank subject against the repository format; it is a local guardrail and is bypassable, not a server-side guarantee. PR title format and meaningful body/footers remain review responsibilities. Review an existing custom hooks path before replacing it.

On Windows, use Git Bash from Git for Windows for the hook. `.gitattributes` preserves its LF line endings. No Node dependency is needed.

## Checks

Run `git diff --check` for edited text. Follow `docs/agents/rust.md` for Cargo checks once the Rust package exists. For Windows behavior, follow `docs/acceptance.md` and record the OS/browser versions tested. Do not claim that a check passed if it was not run.

## Standards

- [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).
- [Git commit-msg hooks](https://git-scm.com/docs/githooks#_commit_msg).
