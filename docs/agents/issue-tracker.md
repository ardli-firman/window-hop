# Issue tracker: GitHub

Specs and tickets live in GitHub Issues for `ardli-firman/window-hop`. Use the `gh` CLI from this clone; the Git remote identifies the repository.

## Read and publish

- Read an issue with `gh issue view <number> --comments`; fetch labels with `gh issue view <number> --json number,title,body,labels,comments` when triaging.
- List work with `gh issue list --state open --json number,title,labels`, filtered by category, state, or document kind as needed.
- Render spec bodies using `.github/ISSUE_TEMPLATE/spec.md` and ticket bodies using `.github/ISSUE_TEMPLATE/ticket.md`. Strip the YAML front matter before passing a body file to `gh`.
- Create with `gh issue create --title "..." --body-file <path> --label <label>`. Use `--body-file` for multiline issue bodies, comments, and PR descriptions so actual newlines and literal text are preserved.
- Update with `gh issue edit <number> --body-file <path>` or `--add-label` / `--remove-label`. Comment with `gh issue comment <number> --body-file <path>`.
- Reuse an existing issue when it already represents the same work. Create one issue per spec or ticket; linking to repo documents provides supporting detail.

## Labels and readiness

Human-authored issue templates start with `enhancement` and `needs-triage`. Specs additionally use `type:spec`; tickets use `type:ticket`.

`to-spec` and `to-tickets` publish approved, agent-ready work with `ready-for-agent` instead of `needs-triage`, preserving the category and document-kind labels. Exactly one triage-state label applies; the mapping is in `triage-labels.md`.

When a skill says "publish to the issue tracker", create a GitHub issue. When it says "fetch the relevant ticket", read the full issue and its comments. Configuring templates does not by itself publish product specs or tickets.

## Parent issues and blockers

Tickets reference their parent spec and list genuine blockers, or `None (can start immediately)`. Publish blockers first so references use real issue numbers. Use native GitHub sub-issues and blocking dependencies where available; otherwise keep `Parent` and `Blocked by` references in the issue body and a child task list in the parent. Native relationship APIs require the issue database ID, not the displayed issue number. Consult current API documentation before writing those relationships.

A ticket is available when every blocking issue is closed. `to-tickets` does not close or modify the parent issue unless the user separately asks for that action.

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub shares issue and PR numbering. If a referenced number is ambiguous, resolve with `gh pr view <number>` and fall back to `gh issue view <number>`. PR work uses `.github/pull_request_template.md` and links its implementation ticket.
