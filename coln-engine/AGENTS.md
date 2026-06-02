# AGENTS.md

This file provides guidance to coding agents collaborating on this repository.

## Mission

Geomerge is an experimental Rust workspace for a storage layer used by the Geolog database engine.

Priorities, in order:

1. Correct storage, validation, and transaction semantics.
2. Clear boundaries between IR, store, solver, persistence, and REPL code.
3. Accurate persistence and reload behaviour.
4. Performance and scalability.
5. Clear, maintainable, idiomatic Rust code.

## Core Rules

- Keep mutable state inside well-defined structs; avoid global mutable state.
- Prefer small, focused changes over broad refactoring.
- Add comments only when they clarify non-obvious behaviour.
- Follow Rust idioms: use `Result` for errors, iterators where they improve clarity, and precise ownership.
- This is a research prototype, so do not worry about backwards compatibility issues, prioritise cleaner design & implementation.

Quick examples:

- Good: add a validation edge-case test in the module that owns the behaviour.
- Good: extend a REPL command by updating parsing, execution, and tests together.
- Good: keep persisted schema data and compiled law data clearly separated.
- Bad: mix REPL parsing, store mutation, and validation logic in one helper.
- Bad: add global configuration that changes unrelated store behavior.

## Writing Style

- Use Oxford commas in inline lists: "a, b, and c" not "a, b, c".
- Do not use em dashes. Restructure the sentence, or use a colon or semicolon.
- Avoid colorful adjectives and adverbs. Write "TCP proxy" not "lightweight TCP proxy", and "scoring components" not "transparent scoring components".
- Use noun phrases for checklist items, not imperative verbs. Write "redundant index detection" not "detect redundant indexes".
- Headings in Markdown files must be in title case. Minor words such as "a", "an", "the", "and", "but", "or", "for", "in", "on", "at", "to", "by",
  and "of" stay lowercase unless they are the first word.

## Repository Layout

- `Cargo.toml`: workspace definition.
- `crates/geolog-lang/`: Geolog IR crate.
    - `src/ir/mod.rs`: schema, table, law, proposition, term, and literal IR types.
    - `src/ir/path.rs`: path parsing, display, and conversion helpers.
- `crates/geomerge/`: store engine crate and binary.
    - `src/lib.rs`: crate exports.
    - `src/main.rs`: REPL entry point.
    - `src/table.rs`: column storage, row ids, cell values, and table validation.
    - `src/store.rs`: table registry, theory loading, law compilation, batch
      application, and whole-store law checks.
    - `src/solver/`: law compilation, matching, binding, and validation.
    - `src/commit/`: logic dealing with commits and graphs
      - `src/commit/wire`: the core encoding/decoding logic
    - `src/repl/`: REPL parsing, execution, summaries, and errors.
    - `src/txn`: Logic dealing with transactions, this includes user-facing transaction API.
    - `examples/`: example Geolog theory files.
    - `tests/`: crate-level integration tests and fixture data.

## Architecture Constraints

- Treat `geolog-lang` as the source of shared IR definitions. Do not duplicate IR shape in `geomerge` unless conversion boundaries require it.
- `Store` owns table registration, table lookup, compiled laws, and a commit graph which is the columnar encoded operations.
- `Table` is the materialised view of what each table should contain, after playing the commits. It also has schema-level validation for inserted values.
- Store mutation should flow through explicit operations such as `Op` and transaction helpers.
- Law compilation and validation logic belongs under `solver`.
- REPL code should stay presentation-oriented: parse commands, call store APIs, and format results.
- Public docs and interfaces should reflect the implemented state of the repository accurately.

## Rust Conventions

- Target stable Rust with edition 2024, as configured by `rust-toolchain.toml` and crate manifests.
- Prefer `&str` over `String` in function parameters when ownership is not needed.
- Use `impl Trait` for return types when the concrete type is an implementation detail.
- Keep error types meaningful and implement `Display` when users can see the error.
- Run clippy and address warnings before committing.

## Required Validation

Run these checks for any non-trivial change:

`just check`

For performance-sensitive changes:

1. Benchmark coverage for the affected path, if benchmarks exist or can be added in a focused way.
2. Before and after performance comparison.

## First Contribution Flow

Use this sequence for your first change:

1. Read `crates/geomerge/src/lib.rs`, `crates/geolog-lang/src/lib.rs`, and the relevant module files.
2. Implement the smallest possible code change.
3. Add or update tests that fail before and pass after.
4. Run `cargo test --workspace --all-targets`.
5. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
6. Run `cargo fmt --all --check`.
7. Update docs if public API or user-facing behavior changed.

Example scopes that are good first tasks:

- A parser or path edge-case test in `geolog-lang`.
- A validation edge-case test in `Table` or `Store`.
- A focused REPL parsing or execution fix.
- A persistence round-trip test for supported store state.
- Tightened README wording so it matches actual behavior.

## Testing Expectations

- No semantics-changing logic update is complete without tests.
- Unit tests go in `#[cfg(test)] mod tests` within each module when the behavior is local to that module.
- Integration tests for `geomerge` go in `crates/geomerge/tests/`.
- Fixture data for those tests goes in `crates/geomerge/tests/data/`.
- Runnable examples belong in `crates/geomerge/examples/` when they clarify supported behavior.
- Do not merge code that breaks existing tests.

Minimal unit-test checklist for store-related behavior:

1. Build or load a `FlatTheory` with relevant tables and laws.
2. Create a `Store` with `Store::try_from_theory`.
3. Apply operations through `Store::apply_batch` or REPL transaction helpers.
4. Assert on returned row ids, table contents, validation errors, and law violations.

## Change Design Checklist

Before coding:

1. Impact on storage semantics, law validation, persistence, or REPL behavior.
2. Affected tests and fixture data.
3. API stability for exported `geomerge` and `geolog-lang` types.
4. Documentation accuracy for implemented features.

Before submitting:

1. Passing `cargo test --workspace --all-targets`.
2. Passing `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
3. Passing `cargo fmt --all --check`.
4. Tests added or updated where behavior changed.
5. Docs updated where public behavior changed.

## Review Guidelines

Review output should be concise and focused on critical issues.

- `P0`: must-fix defects, such as incorrect validation, data loss, corruption, or broken persistence.
- `P1`: high-priority defects, such as likely functional bugs, API breakage, misleading public behavior, or significant performance regression.

Do not include:

- Style-only nitpicks.
- Praise or summary of what is already good.
- Exhaustive restatement of the patch.

Use this review format:

1. `Severity` (`P0` or `P1`)
2. `File:line`
3. `Issue`
4. `Why it matters`
5. `Minimal fix direction`

## Practical Notes for Agents

- Prefer targeted edits over broad mechanical rewrites.
- If repository conventions contradict this file, follow existing code and update this file when appropriate.
- When uncertain about correctness, add or extend tests first, then optimize.
- Keep REPL presentation logic separate from store, solver, and persistence behavior.
- Keep user-facing naming consistent with the repository name: `geomerge`.
- If you change supported REPL commands or persisted formats, update `README.md` and relevant examples in the same change.

## Commit and PR Hygiene

- Keep commits scoped to one logical change.
- PR descriptions should include:
    1. Behavioural change summary.
    2. Tests added or updated.
    3. Performance impact, if applicable.
    4. API changes, if any.
    5. Architecture or persistence impact, if applicable.

Suggested PR checklist:

- [ ] Tests added or updated for behavior changes
- [ ] `cargo test --workspace --all-targets` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt --all --check` passes
