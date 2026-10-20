# Moriq Space third-party components

Moriq Space source is distributed under the Apache License, Version 2.0, in LICENSE. Project attribution is retained in NOTICE. Individual third-party components retain their own licenses. Portable builds generate THIRD_PARTY_LICENSES.md, licenses/, license-gaps.json and sbom.cdx.json from locked dependency metadata. Complete corresponding-source archives and release-signing prerequisites remain required before public release.

- LibRaw 0.22.2: https://www.libraw.org/ . The bundled dynamic library uses the LGPL license option; its copyright and license are included in codecs/licenses. The pinned upstream Windows archive contains the library source and build files: https://www.libraw.org/data/LibRaw-0.22.2-Win64.zip . No LibRaw source modifications are made by Moriq Space.
- FFmpeg n8.1.3-18-g46d899d7a0: https://ffmpeg.org/ . BtbN's LGPL shared build enables version3 and is LGPLv3. License is included in codecs/licenses. Exact binary provenance is in codecs/provenance.json. Upstream revision: https://github.com/FFmpeg/FFmpeg/tree/46d899d7a0 . Build recipes: https://github.com/BtbN/FFmpeg-Builds . A reproducible corresponding-source bundle including enabled dependencies must be archived before public release.
- Node.js 24.16.0: https://nodejs.org/ . The runtime and its bundled dependencies' license notices are in ai/Node-LICENSE.txt.
- Transformers.js 3.8.1: https://github.com/huggingface/transformers.js . Apache-2.0. Package notices are retained in ai/node_modules.
- ONNX Runtime 1.21.0: https://github.com/microsoft/onnxruntime . MIT. Package notices are retained in ai/node_modules.
- sharp 0.34.5 and libvips 8.17.3: Apache-2.0 and LGPL-3.0-or-later, with additional components listed in the package README. LGPLv3/GPLv3 texts are retained under licenses/additional; the complete corresponding-source archive for the enabled dependencies remains a public-release prerequisite.
- cryoglyph 0.1.0: MIT OR Apache-2.0 OR Zlib. The local copy under vendor/cryoglyph preserves upstream source and notices and updates its lru dependency to 0.18.2 for the upstream panic-safety fix. See vendor/cryoglyph/MORIQ_PATCH.md.
- moxcms 0.8.1: https://github.com/awxkee/moxcms . BSD-3-Clause or Apache-2.0.
- resvg/usvg 0.45.1: https://github.com/linebender/resvg . Apache-2.0 or MIT.

Model weights are optional downloads initiated by the user, and are not included in the portable package. workers/ai/models.json pins each model revision, file size and SHA-256. Model source links and license names appear in the model manager. Each downloaded model includes its upstream license where supplied. Chinese-CLIP (OFA-Sys, MIT), Florence-2-base (Microsoft, MIT), YuNet (OpenCV Zoo, MIT), and SFace (OpenCV Zoo, Apache-2.0) are separate from the runtime libraries. Model predictions require user review.

Moriq Space's FFmpeg integration and resource-guard source is under crates/moriq-library/native. Dynamic libraries remain replaceable within their compatible ABI. This application does not require removal of third-party copyright notices or restrict rights granted by their licenses.
