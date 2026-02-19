# Docent: AI-Guided Code Review Walkthrough Tool

## Overview

**Docent** is a terminal-based application that provides guided, narrative-driven walkthroughs of code diffs. It addresses the growing challenge of reviewing large AI-generated or AI-assisted code changes, where traditional top-to-bottom diff review becomes cognitively overwhelming and misses the logical structure of the changes.

The name "docent" refers to a museum guide who walks visitors through exhibits with context and narrative—exactly the experience this tool provides for code review.

## Problem Statement

As AI-assisted coding tools become more prevalent, developers increasingly face large diffs that were not written by a human with a linear thought process. These changes are difficult to review because:

1. **No narrative structure**: Raw diffs present changes file-by-file or line-by-line, not in the logical order they were conceived
2. **Lost intent**: The AI that generated the code understood *why* changes were made, but that context is absent from the diff
3. **Cognitive overload**: Large changes (hundreds or thousands of lines) are exhausting to review top-to-bottom
4. **No prioritization**: Critical logic changes are visually equivalent to boilerplate additions

Existing code review tools (GitHub PR interface, `git diff`, editor integrations) all present diffs in file/line order, forcing reviewers to mentally reconstruct the narrative.

## Solution

Docent uses AI to analyze a diff and reconstruct the logical narrative of changes. It then guides the reviewer through a structured walkthrough:

1. **Semantic chunking**: Changes are grouped by logical unit (e.g., "new data model," "API integration," "test coverage") rather than by file
2. **Prioritized ordering**: Critical changes are presented first; boilerplate and tests come later
3. **Contextual explanations**: Each chunk includes an AI-generated explanation of what changed and why it matters
4. **Interactive exploration**: Reviewers can ask questions, drill into details, and record comments at each step

## Core User Experience

### The Walkthrough Flow

1. User invokes `docent <commit-sha>` (or eventually a PR URL, uncommitted changes, etc.)
2. Docent analyzes the diff and generates a "walkthrough plan"—a sequence of logical steps
3. User sees a **minimap** (outline of all steps) alongside a **diff viewer** showing the current step's changes
4. User navigates step-by-step with vim-style keybindings
5. At each step, user can:
   - Read the AI's explanation
   - View the associated diff hunks
   - **Branch** into a sub-conversation to ask questions or discuss concerns
   - **Record a comment** capturing feedback for that section
6. At the end, user has a structured list of comments (with file/line context) ready to apply elsewhere

### Branching Conversations

A key feature is the ability to "branch" at any step—starting a focused sub-conversation about a specific concern without losing your place in the main walkthrough. For example:

- Reviewing step 3 of 7 (authentication changes)
- User presses `b` to branch: "Why didn't you use JWT here?"
- AI responds with explanation
- User either records a comment or is satisfied
- User presses `Esc` to return to step 3 and continues to step 4

This is similar to threaded comments in GitHub PRs, but integrated into the review flow itself.

### Comment Recording

The primary output of a Docent session is a list of recorded comments. Each comment includes:

- The content (written by the user, possibly informed by the AI conversation)
- File path and line range context
- Which walkthrough step it originated from

These comments can later be:
- Copied and pasted into a PR review
- Automatically posted to GitHub (future feature)
- Saved as a review artifact

## Architecture

### Self-Contained TUI with Optional Editor Integration

After evaluating several architectures, the chosen approach is:

**Primary**: A standalone terminal UI that includes its own diff viewer. The TUI is self-contained—users can complete a full review without any external editor.

**Optional enhancement**: File watching enables integration with any editor. When users want to make edits during review, they open their editor (neovim, VS Code, etc.) in a separate terminal tab. Docent watches the filesystem and automatically refreshes its diff view when files change.

This approach was chosen over alternatives because:

| Alternative | Why Not |
|-------------|---------|
| TUI orchestrates neovim via RPC | Tight coupling, requires neovim running, complex bidirectional communication |
| 100% neovim plugin | Limits audience to vim users, complex Lua development for rich UI |
| TUI spawns embedded neovim | Complex process management, unclear benefit over self-contained viewer |

The file-watching approach provides editor integration without editor lock-in. Non-vim users can use the tool; vim users can optionally edit in vim with changes reflected back.

### Component Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│  DOCENT TUI                                                             │
│                                                                         │
│  ┌─────────────────────┐  ┌───────────────────────────────────────────┐ │
│  │ MINIMAP             │  │ DIFF VIEWER                               │ │
│  │                     │  │                                           │ │
│  │ Overview of all     │  │ Syntax-highlighted diff for current step  │ │
│  │ walkthrough steps   │  │ with vim-style navigation                 │ │
│  │ with completion     │  │                                           │ │
│  │ status indicators   │  │                                           │ │
│  ├─────────────────────┤  │                                           │ │
│  │ AI EXPLANATION      │  │                                           │ │
│  │                     │  │                                           │ │
│  │ Context about the   │  │                                           │ │
│  │ current step        │  │                                           │ │
│  │                     │  │                                           │ │
│  │ THREAD (if active)  │  │                                           │ │
│  │ Branched Q&A        │  │                                           │ │
│  └─────────────────────┘  └───────────────────────────────────────────┘ │
│                                                                         │
│  [n]ext [p]rev [b]ranch [c]omment [/]search [q]uit                      │
└─────────────────────────────────────────────────────────────────────────┘
```

### Data Model

```
Walkthrough
├── source: CommitSha | PRUrl | WorkingDirectory
├── original_diff: string (raw diff content)
├── steps: Step[]
│   ├── id: string
│   ├── title: string (e.g., "Add UserSession model")
│   ├── summary: string (AI-generated explanation)
│   ├── priority: "critical" | "normal" | "minor"
│   ├── hunks: Hunk[]
│   │   ├── file_path: string
│   │   ├── start_line: number
│   │   ├── end_line: number
│   │   └── content: string
│   └── threads: Thread[]
│       ├── messages: Message[] (user questions + AI responses)
│       └── recorded_comment: string | null
├── current_step_index: number
└── status: "in_progress" | "completed"
```

### Input Sources (Prioritized)

1. **v1**: Single local git commit (`docent <sha>`)
2. **Future**: Uncommitted changes (`docent --staged`, `docent --unstaged`)
3. **Future**: GitHub PR (`docent https://github.com/org/repo/pull/123`)
4. **Future**: Arbitrary diff file (`docent --diff file.patch`)

### Output Artifacts

1. **v1**: List of comments displayed in terminal, copyable
2. **Future**: Export to markdown file
3. **Future**: Post directly to GitHub PR as review comments

## Technical Stack

**Language**: Rust

Chosen for:
- Fast startup time (important for CLI tools)
- Snappy UI performance
- Single binary distribution
- Strong ecosystem for the required components

**Key Dependencies**:

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal UI framework |
| `tokio` | Async runtime for concurrent operations |
| `git2` | Git operations (libgit2 bindings) |
| `reqwest` | HTTP client for Claude API |
| `notify` | Cross-platform file watching |
| `syntect` | Syntax highlighting for diff viewer |
| `serde` | Serialization for data model and API |

## Key UX Decisions

### Vim-Style Keybindings

The TUI implements vim-style navigation throughout:

- **Normal mode**: `j`/`k` scroll, `gg`/`G` top/bottom, `/` search, `n`/`N` next/prev match
- **Walkthrough navigation**: `]]`/`[[` or `n`/`p` for next/prev step
- **Actions**: `b` branch, `c` comment, `q` quit
- **Minimap**: `zo`/`zc` expand/collapse sections (if hierarchical)

This provides a familiar experience for terminal-native developers without requiring actual vim.

### AI Model Choice

Docent uses a fresh AI model (Claude) to analyze diffs, rather than requiring the model that generated the code. This means:

- Works for any diff, not just AI-generated ones
- No special integration required with code generation tools
- The AI must infer intent from the code itself (which is also what human reviewers do)

Trade-off: The AI might occasionally misinterpret intent that the original author (human or AI) would know. This is acceptable because the goal is to structure the review, not to be authoritative about intent.

### Persistence (Deferred)

v1 does not persist walkthrough state across sessions. A review must be completed in one sitting.

Future versions could add:
- Suspend/resume (`docent --resume`)
- Named review sessions
- Multi-user review sessions

### Comment Workflow

Comments are recorded during the walkthrough but applied separately. This keeps Docent focused on the review experience and avoids the complexity of:

- Directly editing code during review
- Managing git state
- Posting to external systems

Users export their comments and apply them however they prefer (manual PR comments, direct code edits, etc.).

## Project Phases

### Phase 1: Core TUI Shell
- Basic three-pane layout (minimap, explanation, diff viewer)
- Vim-style navigation within the diff viewer
- Static/mock data for walkthrough steps

### Phase 2: Git Integration
- Parse real git commits using `git2`
- Extract hunks and file changes
- Display actual diff content with syntax highlighting

### Phase 3: AI Walkthrough Generation
- Integrate Claude API
- Send diff to AI, receive structured walkthrough plan
- Display AI-generated step titles and explanations

### Phase 4: Interactive Features
- Step-by-step navigation with AI context
- Branching conversations (threads)
- Comment recording

### Phase 5: File Watching
- Integrate `notify` for filesystem watching
- Detect when files change and refresh diff
- Maintain walkthrough state when underlying files change

### Phase 6: Polish & Distribution
- Error handling and edge cases
- Configuration file support
- Installation instructions (cargo install, homebrew, etc.)
- Documentation

## Open Questions & Future Considerations

### Walkthrough Granularity
How granular should steps be? Options:
- One step per "logical change" (AI decides)
- User-configurable granularity ("give me 5 steps" vs "give me 20")
- Hierarchical (high-level steps that expand into sub-steps)

### Handling Very Large Diffs
For extremely large diffs (thousands of lines), considerations:
- Progressive loading of step details
- Batched AI requests
- Summary-first approach ("here are the 3 most important areas")

### Multi-File Steps
A single logical step might span multiple files. The diff viewer needs to handle:
- Showing multiple file hunks in sequence
- Clear visual separation between files
- Jumping between files within a step

### Comment Formatting
When exporting comments for GitHub, should Docent:
- Use GitHub's line-comment format directly?
- Include walkthrough context ("This was step 3: Authentication")?
- Support different output formats (markdown, JSON, GitHub API)?

### Offline Mode
Could Docent work without an AI backend for basic diff viewing and manual step creation? This might be useful for:
- Environments without API access
- Users who want to structure their own walkthrough
- Fallback when API is unavailable

## Non-Goals for v1

- **Code editing**: Docent is for review, not editing. Users edit in their own editor.
- **Approval workflow**: Docent produces comments, not approvals. Approval happens in GitHub/GitLab.
- **Real-time collaboration**: Single-user tool for now.
- **IDE integration**: Terminal-first. IDE plugins are a separate future project.
- **Git operations**: Docent reads git state but never writes (no commits, no branch switching).

## Success Criteria

Docent is successful if:

1. Reviewing a 500-line AI-generated diff feels less overwhelming than reading raw `git diff` output
2. Users can complete a thorough review faster than without the tool
3. The walkthrough narrative helps users understand *why* changes were made, not just *what* changed
4. Comments recorded during review are useful and actionable
5. Vim-native developers feel at home in the TUI

---

*Document created: February 2025*
*Status: Pre-development brainstorming complete, ready for prototyping*
