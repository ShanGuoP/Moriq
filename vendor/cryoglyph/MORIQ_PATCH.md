# Local dependency patch

Source: cryoglyph 0.1.0 from crates.io, upstream https://github.com/iced-rs/cryoglyph .
All src files and upstream license texts are unchanged.

Only the lru requirement is updated from 0.16 to exactly 0.18.2, to use the upstream
panic-safety fix for RUSTSEC-2026-0253. Development-only dependencies and benchmarks
are omitted. This patch can be removed when iced ships an updated dependency.
