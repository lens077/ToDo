# Desktop host directly reuses the Rust core; Captures live outside repositories

The Tauri host calls `todo-core` directly instead of spawning `todo-cli`, so CLI and GUI share one parsing, locking, and write implementation without process-startup overhead or a second semantic layer. Project Tasks remain authoritative in each repository's Markdown, while personal Captures are authoritative in a global local `inbox.md` and enter a repository only through explicit Triage; this prevents quick thoughts from polluting Git history while preserving the product's local, readable-data promise.
