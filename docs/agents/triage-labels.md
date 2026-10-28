# Triage labels

| Canonical role | GitHub label | Meaning |
| --- | --- | --- |
| `needs-triage` | `needs-triage` | Maintainer evaluation needed. |
| `needs-info` | `needs-info` | Waiting for information from the reporter. |
| `ready-for-agent` | `ready-for-agent` | Fully specified work an agent can implement. |
| `ready-for-human` | `ready-for-human` | Requires human work or judgment. |
| `wontfix` | `wontfix` | Will not be actioned. |

Use exactly one state role on each triaged issue. When changing state, remove the previous state while preserving category and document-kind labels. Surface conflicting states rather than guessing which is authoritative.

Category labels are `bug` and `enhancement`. Document-kind labels `type:spec` and `type:ticket` distinguish planning issues from implementation tickets; they are not triage states. Ordinary issues enter `needs-triage`; approved outputs of `to-spec` and `to-tickets` enter `ready-for-agent`.
