# Architecture and validation

Moriq Space currently ships a Windows x86_64 runtime and packaging workflow.
macOS and Linux source branches remain development targets; this repository does
not yet provide a complete, validated portable runtime/package for them. The UI
supports Simplified Chinese and English, with persisted appearance preferences. Local design and historical audit documents remain ignored.

## Ownership

- `moriq-domain`: serializable types and input validation, with no I/O or UI.
- `moriq-render`: geometry and bounded viewport calculations, with no I/O.
- `moriq-runtime`: shared pinned-runtime integrity verification.
- `moriq-ai`: local model downloads and inference, using the shared verifier.
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

Backups freeze the database and immutable file links before copying media bytes
on a separate worker. The final directory is published only after verification
and an active-library identity check. Active import stages are excluded from the
copy; interrupted imports retain their completion receipts for recovery. Snapshot
metadata work still runs on the writer and scales with database/file counts.

Homepage favorites and category covers use bounded projections without replacing
the gallery query snapshot. Metadata, image projections and the custom library
cover have separate request receipts and refresh paths. Synchronous maintenance
tools and the app worker share one checking and repair engine.

## UI and motion

Window file drops use a short quiet period and one timer to collect the platform's
per-file events. Pending batches retain the receiving library and project, then
reuse the picker import pipeline with managed copies. Cancellation and library
changes invalidate pending drops and timer receipts. Hover feedback preserves the
base widget tree; file inspection stays on the Service worker.

Shared colors, borders and control states live in `moriq-ui/src/theme.rs`.
Navigation transitions preserve the persistent shell and input/scroll state.
Card hover feedback retains only the current and departing asset, reverses from
sampled values and stops requesting frames when settled. Toolbar and favorite
buttons animate their drawn appearance while preserving hit targets, focus and
tooltip coordinates. Recycled asset buttons reset held gestures on identity changes.
Nested surfaces preserve the current backdrop and offset.

The image viewer eases wheel/fit/actual-size camera changes, anchors wheel zoom at
the pointer and lets dragging interrupt at the visible position. Decoder requests
follow the destination viewport; intermediate frames reuse existing GPU images.
Resizing, changing assets, losing focus and reduced motion cancel or settle the
corresponding transient state. Reduced motion is persisted, commits navigation
immediately and retains static feedback without animation redraws.

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

The capture-only `--panel motion-proof --select-first --screen assets` scenario
records the normal fit/actual-size actions and nested surfaces using an isolated
test library. Camera-frame and viewport-request traces distinguish redraws from
decoder admission; screenshots must be visually checked as well.

It is diagnostic tooling, not a format/capacity acceptance test. Public releases
still require the signature, source inventory and clean exact tag in SECURITY.md.

## State and execution boundaries

ViewerState owns fit previews, original-resolution regions, video resources and
receipt generations. Focus, close and clear have distinct policies; every exit
releases the same media resources. SelectionState owns the range anchor and mode;
filter drafts, export options and Synco polling have separate state owners.

Service remains the public coordinator. Catalog, media, import, maintenance,
interop and settings use cases live in separate modules and share the same bounded
writer, read leases and media budget. Background and interactive inference both
use the injected InferenceRunner. Library prepares immutable input and commits
version-bound results without depending on the model runtime. The actual model
and revision travel with the inference receipt; invalid receipts are rejected.
Results for an older display version cannot overwrite current classification.

JobKind, JobState and ModelId encode runtime protocols; existing SQLite strings
are parsed explicitly at the storage boundary. Filter, export, builtin and shortcut
messages carry typed choices independent of translated labels. The job path uses
TaskError to distinguish cancellation, stale libraries, invalid input, storage,
runtime and unavailable-service errors. Cancellation and switching do not count
as scheduler failures; display-only legacy APIs still return text errors.

## Remaining validation

Automated regressions cover cancellation, library/version changes, snapshot
consistency, writer responsiveness and query-snapshot isolation. They do not
replace large-library capacity benchmarks, real-model inference, desktop visual
acceptance or forced-process-termination recovery testing. Semantic-index and
catalog-ordering performance changes require their own measured design work.
