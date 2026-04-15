# Contributing to Muxshed

Thanks for your interest in contributing.

## Getting Started

1. Fork the repo and clone your fork
2. Follow the [README](README.md) to set up your dev environment
3. Create a feature branch from `main`

## Development

```sh
./dev.sh
```

This starts the API (port 8080) and frontend dev server (port 5173). No GStreamer needed -- the stub controller simulates the pipeline.

## Code Style

- **Rust:** `cargo clippy` clean, no `unwrap()` in library code, `tracing` for logging
- **Svelte:** TypeScript, Tailwind CSS, no inline styles
- **Commits:** Conventional commits (`feat:`, `fix:`, `chore:`, `docs:`)

## Before Submitting

```sh
cargo test --workspace
cd web && npx svelte-check
cd web && npm run build
```

## Pull Requests

- Keep PRs focused on a single change
- Include a description of what and why
- Reference any related issues

## License

By contributing, you agree that your contributions will be licensed under the [MPL-2.0](LICENSE).
