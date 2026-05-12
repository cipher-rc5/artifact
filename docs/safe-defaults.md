# Safe Defaults

ARTIFACT is a destructive filesystem utility, so rules are intentionally conservative.

## Defaults

- Delete mode defaults to `trash`.
- Symlink scan traversal is disabled.
- Symlink deletion targets are refused.
- Selected paths are revalidated immediately before deletion.
- A deletion manifest is written before mutation starts.
- Generic directory names require project markers and are not treated as orphaned when markers disappear.

## Rule Confidence

Rules declare confidence in code:

- `High`: strong marker or artifact semantics, such as `node_modules` next to `package.json`.
- `Medium`: common generated output but broader false-positive surface, such as Python virtual environments or `.NET` `bin`/`obj`.
- `Low`: useful cleanup target but context-sensitive, such as Xcode `DerivedData`.

## Orphan Policy

Only rules that explicitly opt in can be shown as orphaned when markers are missing. Generic names such as `target`, `bin`, `obj`, and `venv` remain excluded without markers because they can represent non-disposable user data.

## Permanent Delete

Permanent deletion should be used only when Trash is insufficient. It requires explicit user selection and stronger confirmation text. Production releases should be signed and distributed with checksums so users can trust the binary performing deletion.
