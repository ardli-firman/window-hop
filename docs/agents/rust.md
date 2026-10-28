# Rust coding and module guardrails

These are project conventions for Window Hop, targeting Windows 10/11 x64 with Rust, GPUI, and gpui-kit. The layout below guides future code; it does not mean these modules already exist.

## Module boundaries

Start with one Cargo package. Create modules only when they have behavior to own; introduce a workspace when independent packages genuinely need separate boundaries.

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
src/
  main.rs              # Thin executable startup and dependency wiring.
  lib.rs               # Modules and testable application entry points.
  domain.rs            # Profile, identity, lifecycle state, and pure rules.
  application.rs       # Activation, switching, shutdown, and coordination.
  browsers.rs          # Browser capabilities and isolated launch plans.
  platform.rs          # Platform interface and platform module selection.
  platform/
    windows.rs         # Windows APIs, resources, and OS integration.
  persistence.rs       # Profile/settings storage and schema handling.
  ui.rs                # GPUI views, user commands, and state presentation.
tests/                 # Integration tests of externally observable behavior.
```

Use `name.rs` with `name/child.rs` when a module grows. Prefer descriptive responsibility-based names to generic `utils`, `helpers`, or `common` modules. Keep `main.rs` and `lib.rs` focused on entry points and module declarations.

UI invokes application operations. Application coordinates domain rules and browser/platform/storage capabilities. Domain logic imports neither GPUI nor Windows APIs. Keep abstractions at boundaries that need substitution or own an external resource; ordinary pure functions do not need traits.

Keep items private by default; use `pub(crate)` for internal sharing and expose only a module's intended interface. Build APIs around glossary concepts rather than passing interchangeable strings or integers for profile, monitor, and process identity.

## Errors and ownership

- Model lifecycle states with enums and transitions that distinguish monitor disconnection from crash/manual close. Preserve the agreed reconnect and restart behavior.
- Return `Result` for recoverable failures, adding operation context without losing the underlying cause. Match meaningful error variants; show actionable messages in UI.
- Handle external inputs, browser failures, and missing devices without `unwrap` or `expect`. Reserve panics for proven programmer invariants; tests may use `expect` with a useful failure message.
- Prefer borrowing and owned resource handles to unnecessary shared mutable state. Use `Arc` and locks only when concurrency actually requires them; keep lock scope small.

## Windows and browser boundaries

- Contain Windows-specific code behind `#[cfg(windows)]` boundaries. Non-Windows substitutes must report unavailable capabilities rather than pretending OS operations succeeded.
- Put unsafe FFI in small wrappers with explicit ownership, lifetime, and thread rules. Add a `SAFETY:` comment explaining every unsafe operation's preconditions. Release handles and registrations through RAII; document any unsafe `Send`/`Sync` justification.
- Launch a verified executable with structured arguments. Parse custom parameter text with documented Windows quoting rules; `split_whitespace` is insufficient for quoted paths and URLs. Use direct process APIs or `Command`; keep launch parameters out of shell command strings.
- Browser adapters own reserved session-isolation arguments and validation. Custom arguments must not redirect a profile into the nurse's working browser data.
- Verify process ownership using retained handles/identity and the browser's actual launch behavior. An executable name or a remembered PID alone does not establish ownership. Stop only verified profile-owned processes.
- Keep blocking process waits, filesystem work, and OS calls off the GPUI event loop. Use cancellable work and timed deadlines; deliver results through the UI framework's supported update mechanism.
- Match monitors using recorded identity; ambiguous or missing matches produce a status, never a primary-monitor fallback. Preserve the accepted shutdown deadline and manual recovery semantics.

## Dependencies and stored data

Once Cargo is introduced, pin the validated toolchain in `rust-toolchain.toml` and commit `Cargo.lock` for this desktop application. Select compatible GPUI/gpui-kit versions together and record Windows/GPU validation evidence before claiming support.

Choose dependencies for a concrete requirement; keep enabled features narrow. Keep stored profiles versioned and write updates atomically enough to preserve the last valid configuration on failure. Log operation context while excluding browser cookies, credentials, and sensitive custom arguments.

## Verification

Once a Cargo manifest exists, run the relevant package/feature variants of:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Use explicit supported feature sets instead of `--all-features` when features are mutually exclusive. Document that choice. Scope lint allowances to the smallest justified item with a reason.

Test lifecycle behavior through pure transitions and integration boundaries; use fakes for monitors, process events, and time. Use real Windows/GPU/browser checks for the scenarios in `docs/acceptance.md`. A successful cross-build or mock test does not prove window placement, process isolation, focus behavior, or monitor disconnection. Report which checks ran and which need a Windows machine.

## References

- [Cargo package layout](https://doc.rust-lang.org/cargo/guide/project-layout.html).
- [Rust modules and privacy](https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html).
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- [Rust process command arguments](https://doc.rust-lang.org/std/process/struct.Command.html).
