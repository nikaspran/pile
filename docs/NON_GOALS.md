# Product boundaries

pile is a scratchpad editor, not an IDE.

## No project system

pile does not manage projects, folders, workspaces, or dependencies.
Editing does not depend on a project.

## No language server

pile does not provide completion, diagnostics, go-to-definition, references,
rename, or code actions.

It does provide syntax highlighting, comments, and basic indentation.

## No terminal

pile does not run shells, tasks, builds, or debuggers.

## No save prompts

pile does not ask users to save scratch buffers.
It saves the session in the background.
Files can be imported or exported when needed.

## No file-first workflow

New buffers do not require a path or a name.
Files are for import and export, not the main workflow.

## No collaboration or cloud sync

Buffers remain on the local machine.
pile does not provide shared editing, comments, cloud sync, or version control.

## No plugin system

pile does not load plugins or extensions.

## Feature test

Before adding a feature, check:

1. Does it improve capture, editing, search, or recovery?
2. Does it preserve typing and navigation performance?
3. Does it work without a project or setup step?
4. Does it preserve automatic session recovery?

If not, it is outside the product scope.
