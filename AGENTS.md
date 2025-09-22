# Repository Guidelines

## Build, Run & Dev Commands

- Build app: `cd saba && make build`
  - Adds target `x86_64-unknown-none` and compiles release with required `RUSTFLAGS`.
- Run under QEMU: `cd saba && ./run_on_wasabi.sh`
  - Headless: `HEADLESS=1 ./run_on_wasabi.sh`
  - VNC viewer: `open vnc://localhost:5905`.
- Lint/format: `cargo fmt` and `cargo clippy` (run in `saba/`).
- Prereqs (macOS): `brew install qemu wget jq`.

## Coding Style & Naming Conventions

- Rust 2021; crates prefer `no_std` (OS-like env). Keep `main.rs` thin; move logic to `saba_core`.
- Names: modules/files `snake_case`, types/enums `PascalCase`, functions/vars `snake_case`.
- Formatting: `rustfmt` defaults. Avoid panics on hot paths; prefer `Result` and clear error messages.

## Agent-Specific Instructions (Commenting Rules)

- Write comments for Rust beginners. Prefer short, direct sentences.
- Bridge concepts using TypeScript/Python analogies:
  - “trait ≈ TS interface / Python protocol”; “enum variants ≈ TS union cases”.
  - “`Result<T, E>` ≈ `try/except` outcome; use `?` like `await`/`raise` propagation.”
  - “ownership/borrowing ≈ moving vs. referencing; think mutable vs. readonly refs.”
- Explain `no_std`, `alloc`, and OS-specific macros (e.g., `entry_point!`) where used.
- At module tops, describe purpose, inputs/outputs, and simple examples (one-liners or code blocks).
- Favor examples over theory. Show tiny snippets: paths (`saba_core::url::parse`) and commands.
- Keep comments updated when code changes; outdated comments must be removed or fixed.
