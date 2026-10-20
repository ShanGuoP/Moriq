# Security checks and release inputs

Report security defects privately to the maintainer rather than attaching a
personal library, model credentials or full media paths to a public issue.

Local validation:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo install cargo-audit --locked --root target/audit-tools
./target/audit-tools/bin/cargo-audit.exe audit --db target/advisory-db
```

The packaging script checks a locally installed cargo-audit when available;
public-release mode requires it. Vulnerabilities and unsoundness block packaging.
Unmaintained dependency warnings remain visible in the retained audit JSON. The
2026-10-20 check retains upstream warnings for paste, rustybuzz and ttf-parser;
paste is introduced through the macOS Metal dependency, and is not in the Windows
dependency graph. These warnings are not represented as repaired vulnerabilities.

`vendor/cryoglyph` retains unchanged upstream rendering source and license texts;
only its lru dependency is updated to use the upstream panic-safety repair.

Development packages may contain uncommitted fixes and are explicitly identified
by a source-file fingerprint and dirty marker. Public packages additionally need:

- A clean, exact Git tag and matching source fingerprint.
- A valid Authenticode signature from a certificate controlled by the publisher.
- Retained third-party license texts and no missing-license inventory entries.
- Reviewed corresponding-source bundles for FFmpeg, LibRaw and libvips including
  their enabled dependencies, notices and build recipes.

Place those source bundles under `target/corresponding-sources/`, with a
`verified.json` manifest using format `1` and a `components` array. Each component
has `name`, `revision`, `reviewed_by`, `reviewed_at`,
`complete_with_dependencies: true`, and `files` entries with relative `path` and
`sha256`. The validator verifies containment, file hashes and review fields;
the publisher remains responsible for reviewing the actual source completeness.

The scripts do not create tags, push changes, generate a publisher certificate,
or claim a missing source archive has been reviewed. CI and the local docs
directory remain under the project's existing user-selected policy.
