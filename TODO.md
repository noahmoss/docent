# TODO

## API & Generation Polish

- [ ] Update loading status to show "Calling Claude API..." after parsing
- [ ] Implement 'r' to retry on error screen
- [ ] Handle Ctrl+C during loading (graceful cancellation)
- [ ] Validate hunk coverage - warn if Claude didn't assign all hunks
- [ ] Better error messages (parse rate limit, invalid key, etc.)
- [ ] Truncate/warn for large diffs exceeding context window
- [ ] Support API key in settings.json (not just env var)
- [ ] Timeout handling with user feedback
- [ ] Streaming support - show steps as they're generated

## Future Enhancements

- [ ] vim-tmux-navigator style integration for Ctrl+h/j/k/l pane navigation
- [ ] syntect integration for proper syntax highlighting in diff viewer
- [ ] tree-sitter integration for finer-grained diff units (split hunks at function/class boundaries rather than relying on git's line-based hunks)

## Pain Points

## Ideas

- [ ] Custom color schemes / theming support
- [ ] **Syntax highlighting in diffs** — Use file extension to detect language, apply keyword highlighting on top of diff coloring. In TUI via `syntect`; in neovim plugin essentially free via treesitter extmarks.
- [ ] **Go-to-definition within the diff** — Build a simple symbol index by scanning all hunks for function/class/struct definitions (regex per language). Let users jump to where a symbol is defined elsewhere in the diff (cross-step). No real LSP needed.
- [ ] **Diff gutter sidebar** — Dedicated narrow column with `+`/`-`/`~` icons instead of relying solely on inline prefix coloring. In neovim plugin, use the sign column.
