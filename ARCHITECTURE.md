# Architecture and validation

Moriq Space currently ships a Windows x86_64 runtime and packaging workflow.
macOS and Linux source branches remain development targets; this repository does
not yet provide a complete, validated portable runtime/package for them. The UI
is Simplified Chinese. Local design and historical audit documents remain ignored.

## Ownership

- `moriq-domain`: serializable types and input validation, with no I/O or UI.
- `moriq-render`: geometry and bounded viewport calculations, with no I/O.
- `moriq-ai`: local model downloads, pinned runtime verification and inference.
- `moriq-synco`: same-user loopback transport; no catalog ownership.
- `moriq-library`: blocking persistence, media plans, immutable source versions,
  recovery journals and file containment. SQLite mutations use one writer.
- `moriq-app`: library identity checks, bounded scheduling, shared memory budgets,
  cancellation and system integration outside the SQLite writer.
- `moriq-ui`: Iced presentation, transient view state and request receipts.
  Disk access and media decoding go through Service.
- `apps/desktop`: platform entry point, icon resources and command-line parsing.

A feature must preserve original media, bound work, reject stale library/version
receipts and have failure or cancellation behavior. A compiler/test pass does not
establish real-media format coverage, capacity, frame rate or macOS acceptance.

## Deletion and recovery

Project deletion executes only the complete scope shown at confirmation. Changes
to members, exclusive/shared ownership, versions or child projects require a fresh
confirmation. Committed and rolled-back purge journals retire after listed files
are handled. Unknown files in their trash directory are retained, never recursively
deleted and never used to authorize deleting additional media.

Referenced import stores the external original's identity and checksum. It does
not authorize moving or deleting that original. Missing thumbnail caches rebuild
on a media worker, with decoder admission to the shared memory budget.

## UI and motion

Shared colors, borders and control states live in `moriq-ui/src/theme.rs`.
Navigation transitions preserve the persistent shell and input/scroll state.
Hover feedback owns one transient asset ID, reverses from its current value and
stops requesting animation frames when settled. Reduced motion is a persisted
preference that commits navigation immediately and retains static state feedback.

## Local validation

```powershell
python scripts/check-source-style.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo check --workspace --all-targets --locked
```

The extra style gate checks executable code density because rustfmt can leave
packed macro bodies unchanged. Long literals, SQL and raw SVG data are excluded.
The package script runs it alongside formatting, static checks and tests. The
existing decision to keep GitHub Actions disabled is preserved.

Screenshot automation is opt-in, and excluded from ordinary desktop builds:

```powershell
cargo run -p moriq-desktop --features capture -- --library PATH --screen assets --capture OUTPUT.png --exit-after-capture
```

It is diagnostic tooling, not a format/capacity acceptance test. Public releases
still require the signature, source inventory and clean exact tag in SECURITY.md.

## Remaining architectural work

The audit also proposes reorganizing the application facade, completing UI state
encapsulation, typed filter/shortcut messages, inference injection, persistent
semantic indexes and sparse catalog ordering. These are broader design changes,
not claims of repaired defects or completed performance acceptance in this patch.
