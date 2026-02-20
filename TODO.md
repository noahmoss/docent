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

## Pain Points

## Ideas

- [ ] Custom color schemes / theming support
