## What and why

<!-- What does this change and why? Link issues with "Fixes #123". -->

## Checklist

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` pass
- [ ] Extension changes: `node --test test/` and `web-ext lint` pass
- [ ] Tests added/updated for new logic
- [ ] `CHANGELOG.md` updated under **Unreleased**
- [ ] Protocol changes documented in `docs/PROTOCOL.md` and still compatible with the NDM extension
- [ ] Security impact described if this touches the WebSocket server, aria2 RPC or extension permissions
