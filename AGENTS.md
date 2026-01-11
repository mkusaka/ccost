# Repository Guidelines

## Project Structure & Module Organization
- `src/`: Rust source code. Main entry is `src/main.rs`; CLI and reporting logic live under `src/cli.rs`, `src/data_loader.rs`, `src/pricing.rs`, and helpers.
- `assets/`: Embedded data (e.g., `assets/claude_pricing.json`) used for offline pricing.
- `scripts/`: Maintenance scripts (e.g., `scripts/update_offline_pricing.sh`).
- `.github/workflows/`: CI and automation workflows.
- `target/`: Build artifacts (ignored in reviews).

## Build, Test, and Development Commands
- `cargo build --release`: Build the optimized binary.
- `cargo build`: Debug build for quick iteration.
- `./target/release/ccost daily --json`: Run locally against real data.
- `cargo test`: Run unit tests.
- `cargo clippy -- -D warnings`: Lint with Clippy (treat warnings as errors).
- `cargo fmt`: Format code with rustfmt.
- `scripts/update_offline_pricing.sh`: Refresh embedded pricing data.

## Coding Style & Naming Conventions
- Rust standard style: 4-space indentation, `snake_case` for functions/variables, `CamelCase` for types.
- Prefer `rustfmt` output; keep small, focused functions and clear error handling with `anyhow`.
- CLI flags follow kebab-case (`--offline`, `--since`).

## Testing Guidelines
- Tests are Rust unit tests colocated in modules under `src/` (see `#[cfg(test)]`).
- Use `cargo test` locally; new behavior should include or update tests where applicable.
- Prefer deterministic fixtures for JSONL parsing and date handling.

## Commit & Pull Request Guidelines
- Commit messages are concise, imperative, and scoped (e.g., “Default to daily when no args”).
- PRs should include a short summary, testing notes (commands run), and link related issues if any.
- For output/UI changes, include before/after output snippets.

## Security & Configuration Notes
- Offline pricing is embedded; update via `scripts/update_offline_pricing.sh`.
- Avoid committing user data from `projects` directories or local Claude config paths.
