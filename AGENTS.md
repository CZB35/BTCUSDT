# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust backend prototype plus product documentation for a BTCUSDT event signal platform. The executable lives in `src/main.rs`; it contains the CLI, HTTP/WebSocket service, market-data ingestion, prediction logic, persistence, and API handlers in one binary crate. Architecture and product notes are in `docs/`, numbered by topic from product scope through deployment operations. `Cargo.toml` and `Cargo.lock` define the Rust package and pinned dependencies. `target/` and `.signal-state*` / `.futures-signal-state*` directories are local build or runtime output, not source.

## Build, Test, and Development Commands

- `cargo build` compiles the `event-signal-prototype` binary.
- `cargo run -- --help` shows available CLI flags.
- `cargo run -- --serve --listen 127.0.0.1:8787` starts the HTTP/WebSocket service.
- `cargo fmt -- --check` verifies formatting; use `cargo fmt` to apply standard Rust formatting.
- `cargo clippy -- -D warnings` catches common Rust issues before review.

## Coding Style & Naming Conventions

Use Rust 2021 conventions and `rustfmt` defaults: four-space indentation, `snake_case` for functions and variables, `PascalCase` for types, and `SCREAMING_SNAKE_CASE` for constants. Prefer small, typed helpers over adding more logic to `main.rs` control flow. Keep API request/response structs explicit and derive `Serialize` / `Deserialize` as needed.

## Testing Guidelines

There are no committed test modules in the current snapshot. Validate changes with `cargo check`, `cargo fmt -- --check`, Android Gradle builds, and `npm run build` for the admin UI. Add tests in future work only when the repository reintroduces a dedicated test suite.

## Commit & Pull Request Guidelines

Use concise imperative commits such as `Add settlement persistence` or Conventional Commits like `fix: validate encrypted wallet payload`. Pull requests should describe the behavior change, list verification commands, link related issues or docs, and include API examples or screenshots when user-facing contracts change.

## Security & Configuration Tips

Do not commit secrets, database URLs, private keys, JWT secrets, or generated state files. Service mode reads `DATABASE_URL`, `CORS_ALLOWED_ORIGINS`, `JWT_SECRET`, `APP_ENCRYPTION_KEY`, and BSC verification variables such as `BSC_RPC_URL`. Admin users and sessions live in database tables. The admin frontend reads `VITE_API_BASE_URL`; Android reads `EVENT_SIGNAL_API_BASE_URL` at build time. Keep local values in your shell or ignored environment files. Treat Binance, BSC, wallet, and accounting changes as high-risk and document assumptions in `docs/` when behavior changes.
