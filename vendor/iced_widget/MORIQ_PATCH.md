# Iced widget scrollbar patch

Source: iced_widget 0.14.2 from crates.io, Iced revision
`54abf81d13fec06d4d9ac754b03ce2c3313e8a1f`, `widget/`.
Upstream: https://github.com/iced-rs/iced
License: MIT, preserved in LICENSE.

Moriq changes `src/scrollable.rs` to give the vertical scrollbar thumb a
36 logical pixel minimum height (limited to half the track for very small
viewports). The offset uses the remaining thumb travel, so both endpoints
and pointer dragging stay accurate for virtualized galleries.

The `moriq_scrollbar_tests` module checks geometry and drag endpoints with
short and very tall content. Trailing whitespace in two upstream button
documentation lines is removed. All other widget behavior remains upstream.
