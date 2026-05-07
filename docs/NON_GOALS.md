# Non-Goals

This document defines what `pile` is explicitly NOT, so that feature requests and PRs do not gradually pull the app toward an IDE scope.

## Core Non-Goals

### 1. No Project System

`pile` is not a project editor. It does not:

- Manage project files, workspaces, or sutions
- Require opening a folder or workspace before editing
- Index project-wide symbols, references, or dependencies
- Provide project-aware navigation (go-to-definition across files, find all references)
- Support multi-root workspaces or project templates

**Rationale**: The primary workflow is hundreds of untitled scratch buffers. Adding projects would fundamentally change the product.

### 2. No LSP (Language Server Protocol)

`pile` does not provide:

- Code completion (IntelliSense)
- Go-to-definition or peek definition
- Find all references
- Real-time diagnostics (errors, warnings, hints)
- Rename symbol across files
- Code actions / quick fixes

**What we DO provide instead**: Syntax highlighting via tree-sitter, comment toggling, and basic indentation rules. These are self-contained, do not require external processes, and do not turn `pile` into an IDE.

### 3. No Integrated Terminal

`pile` does not include:

- An embedded terminal emulator
- Shell integration or command palete with terminal output
- Task running or build system integration
- Debugger integration or output panels

**Rationale**: Terminal emulation is a huge surface area that distracts from the scratchpad use case. Users who need terminals have them outside `pile`.

### 4. No Manual Save Prompts

`pile` will never:

- Ask "Do you want to save?" when closing a tab or the app
- Show save dialogs for scratch buffers
- Require naming documents before editing
- Block exit for unsaved changes

**How persistence works instead**: Everything is auto-saved to a session file in the background. The UI never blocks on I/O. Users can manually export files via `Import/Export` commands, but the core workflow assumes unsaved scratch buffers.

### 5. No File-First Workflow

`pile` does not:

- Require choosing where to save before editing
- Show file paths in tab titles (unless manually renamed)
- Make "Open File" the primary way to start editing
- Treat "Save" as a core workflow step

**What we DO provide**: Native file import/export commands for when users want to load/save files, but the default state is an empty scratch buffer.

### 6. No Collaborative Editing

`pile` does not support:

- Real-time collaborative editing (CRDTs, OT, etc.)
- Comments or review threads
- Version control integration (git diff, blame, etc.)
- Cloud sync or sharing

**Rationale**: The scratchpad use case is personal and local.

### 7. No Plugin/Extension System

`pile` does not provide:

- Plugin APIs or extension points
- Marketplace or plugin manager
- User-installable themes (bundled themes only)
- Custom syntax definitions via user config (use `GrammarRegistry` in code instead)

**Rationale**: Keep the surface area small. Users who need extensibility should fork the code.

## What `pile` IS

To clarify by contrast, `pile` IS:

- A fast, native scratchpad for dumping and recovering notes
- Optimized for hundreds of open unsaved buffers
- Reliable across crashes and restarts
- Low-latency typing and navigation
- Mixed prose/code with content-aware highlighting
- Self-contained: no external dependencies beyond system libraries

## Feature Request Filter

When evaluating feature requests, ask:

1. Does this require a project system? → **No**
2. Does this require LSP or language servers? → **No**
3. Does this add a terminal or build system? → **No**
4. Does this add save prompts or file-first workflow? → **No**
5. Does this add collaboration or cloud features? → **No**
6. Does this add a plugin system? → **No**
7. Does this pull the app toward IDE scope? → **No**

If the answer is "yes" to any of these, the feature is out of scope for `pile`.
