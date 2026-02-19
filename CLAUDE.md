# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Docent is a terminal-based AI-guided code review walkthrough tool written in Rust. It analyzes git diffs and presents them as a structured, narrative-driven walkthrough rather than raw file-by-file changes. Currently in Phase 1 (Core TUI Shell) with mock data.

## Build Commands

```bash
cargo build           # Build the project
cargo run             # Run the TUI application
cargo test            # Run tests
cargo clippy          # Run linter
cargo fmt             # Format code
```

## Architecture

The application follows a standard TUI architecture with separation between state, input handling, and rendering:

```
src/
├── main.rs           # Terminal setup, event loop
├── app.rs            # Application state (App struct), mode management
├── input.rs          # Keyboard/mouse event handling (vim-style)
├── model/            # Data structures
│   ├── mod.rs
│   └── walkthrough.rs  # Walkthrough, Step, Hunk, Message types
├── ui/               # Rendering modules (ratatui)
│   ├── mod.rs        # Layout orchestration (3-pane layout)
│   ├── minimap.rs    # Step overview panel
│   ├── explanation.rs # AI explanation + chat panel
│   └── diff_viewer.rs # Syntax-highlighted diff display
└── colors.rs         # Color constants for theming
```

### Key Patterns

- **Three-pane layout**: Left side split into minimap (40%) and explanation/chat (60%); right side is diff viewer (70% of total width)
- **Vim-style modal input**: Normal mode for navigation, Insert mode for chat input
- **Active pane tracking**: `ActivePane` enum (Minimap, Chat, Diff) determines which pane receives input
- **Pending key sequences**: InputHandler tracks partial vim commands (gg, [[, ]])

### Data Flow

1. `Walkthrough` contains ordered `Step`s, each with `Hunk`s (diff chunks) and `Message`s (chat history)
2. `App` holds the walkthrough plus UI state (current step, scroll positions, input buffer)
3. `InputHandler` processes events and mutates `App`
4. UI modules read `App` state immutably during render

## Key Dependencies

- `ratatui` - Terminal UI framework
- `crossterm` - Terminal event handling
- `serde` - Serialization for data model

## Navigation Keybindings

- `j/k` - Scroll diff
- `n/p` or `]]/[[` - Next/previous step
- `gg/G` - Top/bottom of diff
- `Ctrl+d/u` - Half-page scroll
- `Tab/Shift+Tab` - Cycle panes
- `i` - Enter chat insert mode
- `Enter` - Mark step complete and advance
- `q` - Quit
