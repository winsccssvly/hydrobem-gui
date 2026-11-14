# Third-party notices

The wheel `hydrobem-gui` holds the Python package `hydrobem_gui` and, in its `bin` folder,
`HydroBEM.exe` (the GUI with the Rust crates below compiled into it, statically linked) and the
HydroBEM engine `hydrobem.dll` (with the crates compiled into it: `licenses/hydrobem/`). numpy is
a dependency installed by pip, not shipped. Each keeps its own licence; `cargo xtask dist` copies
their licence files to `dist/licenses/crates/<crate>-<version>/`, and the wheel carries them in
its `.dist-info/licenses/`. This file is written by
`cargo xtask licenses` from the run-time dependency graph (`cargo metadata`, Windows x64);
rerun it after a dependency changes.

| Component | Version | Licence | Use |
|---|---|---|---|
| HydroBEM engine `hydrobem.dll` | ABI 8 (`v0.3.0`) | Apache-2.0 (Copyright 2026 Sanghun Lee) | Built by HydroBEM, imported into `engine/` by `scripts/import-engine.ps1`, installed beside `HydroBEM.exe`; its `LICENSE`, `NOTICE` and third-party notices (with the crates compiled into it) are installed to `licenses/hydrobem/` |
| Visual C++ runtime (`vcruntime140.dll`), Universal C Runtime | system | Microsoft | Linked as DLLs, not distributed: Python's (it puts them first on the GUI's PATH) and Windows' |
| Segoe UI, Consolas, Segoe UI Symbol, Segoe UI Emoji, Malgun Gothic | system | Microsoft (Windows fonts) | Read from the Windows fonts folder at run time, not distributed: HydroBEM.exe carries no font |
| HydroBEM user documentation (`docs/python/`) | HydroBEM `2def9bb` | Apache-2.0 (Copyright 2026 Sanghun Lee) | Copied from HydroBEM's `docs/guide`, `docs/reference` and `docs/references.md` by `scripts/copy_hydrobem_docs.py` (the package renamed, this wheel's installation); not in the wheel |
| HydroBEM examples (`examples/`) | HydroBEM `2def9bb` | Apache-2.0 (Copyright 2026 Sanghun Lee) | Copied from HydroBEM's `examples/` (importing `hydrobem_gui`); installed into `dist/examples/` and the wheel |
| HydroBEM Python package and tests (`python/hydrobem_gui`, `python/tests`, `scripts/gen_model_classes.py`, `scripts/gen_cylinder_reference.py`, `scripts/gen_cylinder_array_reference.py`, `scripts/gen_sphere_array_reference.py`) | HydroBEM `2def9bb` (first copied at `34adff1`) | Apache-2.0 (Copyright 2026 Sanghun Lee) | Copied from HydroBEM's `python/hydrobem`, `tests/python`, `tests/data`; the Python API of the wheel |
| Hulme (1982), J. Fluid Mech. 121, tables 1 and 2 (`python/tests/data/hulme1982*.txt`) | 1982 | Published values of their authors, not licensed by this project (`NOTICE`) | Reference values of the Python tests; not in the wheel |
| Kim and Yue (1990), J. Fluid Mech. 211, table 3 (`python/tests/data/kim_yue1990.txt`) | 1990 | Published values of their authors, not licensed by this project (`NOTICE`) | Reference values of the Python tests; not in the wheel |
| [`accesskit`](https://github.com/AccessKit/accesskit) | 0.25.2 | MIT OR Apache-2.0 | Compiled in |
| [`accesskit_consumer`](https://github.com/AccessKit/accesskit) | 0.39.2 | MIT OR Apache-2.0 | Compiled in |
| [`accesskit_windows`](https://github.com/AccessKit/accesskit) | 0.35.2 | MIT OR Apache-2.0 | Compiled in |
| [`accesskit_winit`](https://github.com/AccessKit/accesskit) | 0.34.2 | Apache-2.0 | Compiled in |
| [`adler2`](https://github.com/oyvindln/adler2) | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | Compiled in |
| [`ahash`](https://github.com/tkaitchuck/ahash) | 0.8.12 | MIT OR Apache-2.0 | Compiled in |
| [`allocator-api2`](https://github.com/zakarumych/allocator-api2) | 0.2.21 | MIT OR Apache-2.0 | Compiled in |
| [`arboard`](https://github.com/1Password/arboard) | 3.7.0 | MIT OR Apache-2.0 | Compiled in |
| [`arrayvec`](https://github.com/bluss/arrayvec) | 0.7.8 | MIT OR Apache-2.0 | Compiled in |
| [`ash`](https://github.com/ash-rs/ash) | 0.38.0+1.3.281 | MIT OR Apache-2.0 | Compiled in |
| [`bit-set`](https://github.com/contain-rs/bit-set) | 0.10.0 | Apache-2.0 OR MIT | Compiled in |
| [`bit-vec`](https://github.com/contain-rs/bit-vec) | 0.9.1 | Apache-2.0 OR MIT | Compiled in |
| [`bitflags`](https://github.com/bitflags/bitflags) | 2.13.2 | MIT OR Apache-2.0 | Compiled in |
| [`bytemuck`](https://github.com/Lokathor/bytemuck) | 1.25.2 | Zlib OR Apache-2.0 OR MIT | Compiled in |
| [`bytemuck_derive`](https://github.com/Lokathor/bytemuck) | 1.12.1 | Zlib OR Apache-2.0 OR MIT | Compiled in |
| [`byteorder-lite`](https://github.com/image-rs/byteorder-lite) | 0.1.0 | Unlicense OR MIT | Compiled in |
| [`cfg-if`](https://github.com/rust-lang/cfg-if) | 1.0.5 | MIT OR Apache-2.0 | Compiled in |
| [`clipboard-win`](https://github.com/DoumanAsh/clipboard-win) | 5.4.3 | BSL-1.0 | Compiled in |
| [`codespan-reporting`](https://github.com/brendanzab/codespan) | 0.13.1 | Apache-2.0 | Compiled in |
| [`color`](https://github.com/linebender/color) | 0.3.3 | Apache-2.0 OR MIT | Compiled in |
| [`crc32fast`](https://github.com/srijs/rust-crc32fast) | 1.5.3 | MIT OR Apache-2.0 | Compiled in |
| [`cursor-icon`](https://github.com/rust-windowing/cursor-icon) | 1.2.0 | MIT OR Apache-2.0 OR Zlib | Compiled in |
| [`delaunator`](https://github.com/mourner/delaunator-rs) | 1.1.0 | ISC | Compiled in |
| [`displaydoc`](https://github.com/yaahc/displaydoc) | 0.2.7 | MIT OR Apache-2.0 | Compiled in |
| [`document-features`](https://github.com/slint-ui/document-features) | 0.2.12 | MIT OR Apache-2.0 | Compiled in |
| [`dpi`](https://github.com/rust-windowing/winit) | 0.1.2 | Apache-2.0 AND MIT | Compiled in |
| [`ecolor`](https://github.com/emilk/egui) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`eframe`](https://github.com/emilk/egui/tree/main/crates/eframe) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`egui`](https://github.com/emilk/egui) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`egui-wgpu`](https://github.com/emilk/egui/tree/main/crates/egui-wgpu) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`egui-winit`](https://github.com/emilk/egui/tree/main/crates/egui-winit) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`egui_extras`](https://github.com/emilk/egui) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`egui_plot`](https://github.com/emilk/egui_plot) | 0.38.0 | MIT OR Apache-2.0 | Compiled in |
| [`either`](https://github.com/rayon-rs/either) | 1.19.0 | MIT OR Apache-2.0 | Compiled in |
| [`emath`](https://github.com/emilk/egui/tree/main/crates/emath) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`enum-map`](https://codeberg.org/sugar700/enum-map) | 3.1.0 | MIT OR Apache-2.0 | Compiled in |
| [`enum-map-derive`](https://codeberg.org/sugar700/enum-map) | 1.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`enumn`](https://github.com/dtolnay/enumn) | 0.1.14 | MIT OR Apache-2.0 | Compiled in |
| [`epaint`](https://github.com/emilk/egui/tree/main/crates/epaint) | 0.37.0 | MIT OR Apache-2.0 | Compiled in |
| [`equivalent`](https://github.com/indexmap-rs/equivalent) | 1.0.3 | Apache-2.0 OR MIT | Compiled in |
| [`error-code`](https://github.com/DoumanAsh/error-code) | 3.4.0 | BSL-1.0 | Compiled in |
| [`euclid`](https://github.com/servo/euclid) | 0.22.14 | MIT OR Apache-2.0 | Compiled in |
| [`fax`](https://github.com/pdf-rs/fax) | 0.2.7 | MIT | Compiled in |
| [`fdeflate`](https://github.com/image-rs/fdeflate) | 0.3.7 | MIT OR Apache-2.0 | Compiled in |
| [`fearless_simd`](https://github.com/linebender/fearless_simd) | 0.4.1 | Apache-2.0 OR MIT | Compiled in |
| [`flate2`](https://github.com/rust-lang/flate2-rs) | 1.1.10 | MIT OR Apache-2.0 | Compiled in |
| [`foldhash`](https://github.com/orlp/foldhash) | 0.2.0 | Zlib | Compiled in |
| [`font-types`](https://github.com/googlefonts/fontations) | 0.12.7 | MIT OR Apache-2.0 | Compiled in |
| [`form_urlencoded`](https://github.com/servo/rust-url) | 1.2.2 | MIT OR Apache-2.0 | Compiled in |
| [`getrandom`](https://github.com/rust-random/getrandom) | 0.3.4 | MIT OR Apache-2.0 | Compiled in |
| [`glifo`](https://github.com/linebender/vello) | 0.3.0 | Apache-2.0 OR MIT | Compiled in |
| [`glow`](https://github.com/grovesNL/glow) | 0.17.0 | MIT OR Apache-2.0 OR Zlib | Compiled in |
| [`glutin_wgl_sys`](https://github.com/rust-windowing/glutin) | 0.6.1 | Apache-2.0 | Compiled in |
| [`gpu-allocator`](https://github.com/Traverse-Research/gpu-allocator) | 0.28.0 | MIT OR Apache-2.0 | Compiled in |
| [`guillotiere`](https://github.com/nical/guillotiere) | 0.7.0 | MIT/Apache-2.0 | Compiled in |
| [`half`](https://github.com/VoidStarKat/half-rs) | 2.7.1 | MIT OR Apache-2.0 | Compiled in |
| [`harfrust`](https://github.com/harfbuzz/harfrust) | 0.12.0 | MIT | Compiled in |
| [`hashbrown`](https://github.com/rust-lang/hashbrown) | 0.16.1 | MIT OR Apache-2.0 | Compiled in |
| [`hashbrown`](https://github.com/rust-lang/hashbrown) | 0.17.1 | MIT OR Apache-2.0 | Compiled in |
| [`icu_collections`](https://github.com/unicode-org/icu4x) | 2.4.0 | Unicode-3.0 | Compiled in |
| [`icu_locale_core`](https://github.com/unicode-org/icu4x) | 2.4.0 | Unicode-3.0 | Compiled in |
| [`icu_normalizer`](https://github.com/unicode-org/icu4x) | 2.4.0 | Unicode-3.0 | Compiled in |
| [`icu_normalizer_data`](https://github.com/unicode-org/icu4x) | 2.4.0 | Unicode-3.0 | Compiled in |
| [`icu_properties`](https://github.com/unicode-org/icu4x) | 2.4.0 | Unicode-3.0 | Compiled in |
| [`icu_properties_data`](https://github.com/unicode-org/icu4x) | 2.4.0 | Unicode-3.0 | Compiled in |
| [`icu_provider`](https://github.com/unicode-org/icu4x) | 2.4.0 | Unicode-3.0 | Compiled in |
| [`idna`](https://github.com/servo/rust-url/) | 1.1.0 | MIT OR Apache-2.0 | Compiled in |
| [`idna_adapter`](https://github.com/hsivonen/idna_adapter) | 1.2.2 | Apache-2.0 OR MIT | Compiled in |
| [`image`](https://github.com/image-rs/image) | 0.25.10 | MIT OR Apache-2.0 | Compiled in |
| [`indexmap`](https://github.com/indexmap-rs/indexmap) | 2.14.2 | Apache-2.0 OR MIT | Compiled in |
| [`itertools`](https://github.com/rust-itertools/itertools) | 0.15.0 | MIT OR Apache-2.0 | Compiled in |
| [`itoa`](https://github.com/dtolnay/itoa) | 1.0.18 | MIT OR Apache-2.0 | Compiled in |
| [`khronos-egl`](https://github.com/timothee-haudebourg/khronos-egl) | 6.0.0 | MIT/Apache-2.0 | Compiled in |
| [`kurbo`](https://github.com/linebender/kurbo) | 0.13.1 | Apache-2.0 OR MIT | Compiled in |
| [`libc`](https://github.com/rust-lang/libc) | 0.2.191 | MIT OR Apache-2.0 | Compiled in |
| [`libloading`](https://github.com/nagisa/rust_libloading/) | 0.8.9 | ISC | Compiled in |
| [`libm`](https://github.com/rust-lang/compiler-builtins) | 0.2.16 | MIT | Compiled in |
| [`linebender_resource_handle`](https://github.com/linebender/raw_resource_handle) | 0.1.1 | Apache-2.0 OR MIT | Compiled in |
| [`litemap`](https://github.com/unicode-org/icu4x) | 0.8.3 | Unicode-3.0 | Compiled in |
| [`litrs`](https://github.com/LukasKalbertodt/litrs) | 1.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`lock_api`](https://github.com/Amanieu/parking_lot) | 0.4.14 | MIT OR Apache-2.0 | Compiled in |
| [`log`](https://github.com/rust-lang/log) | 0.4.35 | MIT OR Apache-2.0 | Compiled in |
| [`memchr`](https://github.com/BurntSushi/memchr) | 2.8.3 | Unlicense OR MIT | Compiled in |
| [`mime`](https://github.com/hyperium/mime) | 0.3.17 | MIT OR Apache-2.0 | Compiled in |
| [`mime_guess2`](https://github.com/ttys3/mime_guess2) | 2.3.1 | MIT | Compiled in |
| [`miniz_oxide`](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | 0.8.9 | MIT OR Zlib OR Apache-2.0 | Compiled in |
| [`miniz_oxide`](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | 0.9.1 | MIT OR Zlib OR Apache-2.0 | Compiled in |
| [`moxcms`](https://github.com/awxkee/moxcms.git) | 0.8.1 | BSD-3-Clause OR Apache-2.0 | Compiled in |
| [`naga`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`naga-types`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`nohash-hasher`](https://github.com/paritytech/nohash-hasher) | 0.2.0 | Apache-2.0 OR MIT | Compiled in |
| [`num-complex`](https://github.com/rust-num/num-complex) | 0.4.6 | MIT OR Apache-2.0 | Compiled in |
| [`num-traits`](https://github.com/rust-num/num-traits) | 0.2.19 | MIT OR Apache-2.0 | Compiled in |
| [`once_cell`](https://github.com/matklad/once_cell) | 1.21.4 | MIT OR Apache-2.0 | Compiled in |
| [`ordered-float`](https://github.com/reem/rust-ordered-float) | 5.5.1 | MIT | Compiled in |
| [`parking_lot`](https://github.com/Amanieu/parking_lot) | 0.12.5 | MIT OR Apache-2.0 | Compiled in |
| [`parking_lot_core`](https://github.com/Amanieu/parking_lot) | 0.9.12 | MIT OR Apache-2.0 | Compiled in |
| [`peniko`](https://github.com/linebender/peniko) | 0.6.1 | Apache-2.0 OR MIT | Compiled in |
| [`percent-encoding`](https://github.com/servo/rust-url/) | 2.3.2 | MIT OR Apache-2.0 | Compiled in |
| [`pin-project-lite`](https://github.com/taiki-e/pin-project-lite) | 0.2.17 | Apache-2.0 OR MIT | Compiled in |
| [`png`](https://github.com/image-rs/image-png) | 0.18.1 | MIT OR Apache-2.0 | Compiled in |
| [`pollster`](https://github.com/zesterer/pollster) | 1.0.1 | Apache-2.0/MIT | Compiled in |
| [`polycool`](https://github.com/linebender/kurbo) | 0.4.0 | MIT OR Apache-2.0 | Compiled in |
| [`potential_utf`](https://github.com/unicode-org/icu4x) | 0.1.6 | Unicode-3.0 | Compiled in |
| [`presser`](https://github.com/EmbarkStudios/presser) | 0.3.1 | MIT OR Apache-2.0 | Compiled in |
| [`proc-macro2`](https://github.com/dtolnay/proc-macro2) | 1.0.107 | MIT OR Apache-2.0 | Compiled in |
| [`profiling`](https://github.com/aclysma/profiling) | 1.0.18 | MIT OR Apache-2.0 | Compiled in |
| [`pxfm`](https://github.com/awxkee/pxfm) | 0.1.30 | BSD-3-Clause OR Apache-2.0 | Compiled in |
| [`quick-error`](http://github.com/tailhook/quick-error) | 2.0.1 | MIT/Apache-2.0 | Compiled in |
| [`quote`](https://github.com/dtolnay/quote) | 1.0.47 | MIT OR Apache-2.0 | Compiled in |
| [`range-alloc`](https://github.com/gfx-rs/range-alloc) | 0.1.5 | MIT OR Apache-2.0 | Compiled in |
| [`raw-window-handle`](https://github.com/rust-windowing/raw-window-handle) | 0.6.2 | MIT OR Apache-2.0 OR Zlib | Compiled in |
| [`read-fonts`](https://github.com/googlefonts/fontations) | 0.41.0 | MIT OR Apache-2.0 | Compiled in |
| [`renderdoc-sys`](https://github.com/ebkalderon/renderdoc-rs) | 1.1.0 | MIT OR Apache-2.0 | Compiled in |
| [`rfd`](https://github.com/PolyMeilex/rfd) | 0.17.2 | MIT | Compiled in |
| [`robust`](https://github.com/georust/robust) | 1.2.0 | MIT OR Apache-2.0 | Compiled in |
| [`ron`](https://github.com/ron-rs/ron) | 0.12.3 | MIT OR Apache-2.0 | Compiled in |
| [`rustc-hash`](https://github.com/rust-lang-nursery/rustc-hash) | 1.1.0 | Apache-2.0/MIT | Compiled in |
| [`rustc-hash`](https://github.com/rust-lang/rustc-hash) | 2.1.3 | Apache-2.0 OR MIT | Compiled in |
| [`scopeguard`](https://github.com/bluss/scopeguard) | 1.2.0 | MIT OR Apache-2.0 | Compiled in |
| [`self_cell`](https://github.com/Voultapher/self_cell) | 1.3.0 | Apache-2.0 OR GPL-2.0-only | Compiled in |
| [`serde`](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | Compiled in |
| [`serde_core`](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | Compiled in |
| [`serde_derive`](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | Compiled in |
| [`serde_json`](https://github.com/serde-rs/json) | 1.0.154 | MIT OR Apache-2.0 | Compiled in |
| [`simd-adler32`](https://github.com/mcountryman/simd-adler32) | 0.3.10 | MIT | Compiled in |
| [`skrifa`](https://github.com/googlefonts/fontations) | 0.44.0 | MIT OR Apache-2.0 | Compiled in |
| [`smallvec`](https://github.com/servo/rust-smallvec) | 1.16.4 | MIT OR Apache-2.0 | Compiled in |
| [`smol_str`](https://github.com/rust-analyzer/smol_str) | 0.2.2 | MIT OR Apache-2.0 | Compiled in |
| [`spirv`](https://github.com/gfx-rs/rspirv) | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | Compiled in |
| [`stable_deref_trait`](https://github.com/storyyeller/stable_deref_trait) | 1.2.1 | MIT OR Apache-2.0 | Compiled in |
| [`static_assertions`](https://github.com/nvzqz/static-assertions-rs) | 1.1.0 | MIT OR Apache-2.0 | Compiled in |
| [`syn`](https://github.com/dtolnay/syn) | 2.0.119 | MIT OR Apache-2.0 | Compiled in |
| [`syn`](https://github.com/dtolnay/syn) | 3.0.9 | MIT OR Apache-2.0 | Compiled in |
| [`synstructure`](https://github.com/mystor/synstructure) | 0.14.0 | MIT | Compiled in |
| [`termcolor`](https://github.com/BurntSushi/termcolor) | 1.4.1 | Unlicense OR MIT | Compiled in |
| [`thiserror`](https://github.com/dtolnay/thiserror) | 2.0.21 | MIT OR Apache-2.0 | Compiled in |
| [`thiserror-impl`](https://github.com/dtolnay/thiserror) | 2.0.21 | MIT OR Apache-2.0 | Compiled in |
| [`tiff`](https://github.com/image-rs/image-tiff) | 0.11.3 | MIT | Compiled in |
| [`tinystr`](https://github.com/unicode-org/icu4x) | 0.8.5 | Unicode-3.0 | Compiled in |
| [`tracing`](https://github.com/tokio-rs/tracing) | 0.1.44 | MIT | Compiled in |
| [`tracing-attributes`](https://github.com/tokio-rs/tracing) | 0.1.31 | MIT | Compiled in |
| [`tracing-core`](https://github.com/tokio-rs/tracing) | 0.1.36 | MIT | Compiled in |
| [`type-map`](https://github.com/kardeiz/type-map) | 0.5.1 | MIT/Apache-2.0 | Compiled in |
| [`typeid`](https://github.com/dtolnay/typeid) | 1.0.3 | MIT OR Apache-2.0 | Compiled in |
| [`unicase`](https://github.com/seanmonstar/unicase) | 2.10.0 | MIT OR Apache-2.0 | Compiled in |
| [`unicode-general-category`](https://github.com/yeslogic/unicode-general-category) | 1.1.0 | Apache-2.0 | Compiled in |
| [`unicode-ident`](https://github.com/dtolnay/unicode-ident) | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | Compiled in |
| [`unicode-properties`](https://github.com/unicode-rs/unicode-properties) | 0.1.5 | (MIT OR Apache-2.0) AND Unicode-3.0 | Compiled in |
| [`unicode-segmentation`](https://github.com/unicode-rs/unicode-segmentation) | 1.13.3 | MIT OR Apache-2.0 | Compiled in |
| [`unicode-width`](https://github.com/unicode-rs/unicode-width) | 0.2.3 | MIT OR Apache-2.0 | Compiled in |
| [`url`](https://github.com/servo/rust-url) | 2.5.8 | MIT OR Apache-2.0 | Compiled in |
| [`utf8_iter`](https://github.com/hsivonen/utf8_iter) | 1.1.0 | Apache-2.0 OR MIT | Compiled in |
| [`utf_types`](https://github.com/hsivonen/utf_types) | 1.0.0 | Apache-2.0 OR MIT | Compiled in |
| [`uuid`](https://github.com/uuid-rs/uuid) | 1.29.0 | Apache-2.0 OR MIT | Compiled in |
| [`vello_common`](https://github.com/linebender/vello) | 0.2.0 | Apache-2.0 OR MIT | Compiled in |
| [`vello_cpu`](https://github.com/linebender/vello) | 0.2.0 | Apache-2.0 OR MIT | Compiled in |
| [`web-time`](https://github.com/daxpedda/web-time) | 1.1.0 | MIT OR Apache-2.0 | Compiled in |
| [`webbrowser`](https://github.com/amodm/webbrowser-rs) | 1.2.6 | MIT OR Apache-2.0 | Compiled in |
| [`weezl`](https://github.com/image-rs/weezl) | 0.1.12 | MIT OR Apache-2.0 | Compiled in |
| [`wgpu`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`wgpu-core`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`wgpu-core-deps-windows-linux-android`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`wgpu-hal`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`wgpu-naga-bridge`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`wgpu-types`](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | Compiled in |
| [`winapi-util`](https://github.com/BurntSushi/winapi-util) | 0.1.11 | Unlicense OR MIT | Compiled in |
| [`windows`](https://github.com/microsoft/windows-rs) | 0.62.2 | MIT OR Apache-2.0 | Compiled in |
| [`windows-collections`](https://github.com/microsoft/windows-rs) | 0.3.2 | MIT OR Apache-2.0 | Compiled in |
| [`windows-core`](https://github.com/microsoft/windows-rs) | 0.62.2 | MIT OR Apache-2.0 | Compiled in |
| [`windows-future`](https://github.com/microsoft/windows-rs) | 0.3.2 | MIT OR Apache-2.0 | Compiled in |
| [`windows-implement`](https://github.com/microsoft/windows-rs) | 0.60.2 | MIT OR Apache-2.0 | Compiled in |
| [`windows-interface`](https://github.com/microsoft/windows-rs) | 0.59.3 | MIT OR Apache-2.0 | Compiled in |
| [`windows-link`](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0 | Compiled in |
| [`windows-numerics`](https://github.com/microsoft/windows-rs) | 0.3.1 | MIT OR Apache-2.0 | Compiled in |
| [`windows-result`](https://github.com/microsoft/windows-rs) | 0.4.1 | MIT OR Apache-2.0 | Compiled in |
| [`windows-strings`](https://github.com/microsoft/windows-rs) | 0.5.1 | MIT OR Apache-2.0 | Compiled in |
| [`windows-sys`](https://github.com/microsoft/windows-rs) | 0.52.0 | MIT OR Apache-2.0 | Compiled in |
| [`windows-sys`](https://github.com/microsoft/windows-rs) | 0.61.2 | MIT OR Apache-2.0 | Compiled in |
| [`windows-targets`](https://github.com/microsoft/windows-rs) | 0.52.6 | MIT OR Apache-2.0 | Compiled in |
| [`windows-threading`](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0 | Compiled in |
| [`windows_x86_64_msvc`](https://github.com/microsoft/windows-rs) | 0.52.6 | MIT OR Apache-2.0 | Compiled in |
| [`winit`](https://github.com/rust-windowing/winit) | 0.30.13 | Apache-2.0 | Compiled in |
| [`writeable`](https://github.com/unicode-org/icu4x) | 0.6.4 | Unicode-3.0 | Compiled in |
| [`yoke`](https://github.com/unicode-org/icu4x) | 0.8.4 | Unicode-3.0 | Compiled in |
| [`yoke-derive`](https://github.com/unicode-org/icu4x) | 0.8.4 | Unicode-3.0 | Compiled in |
| [`zerocopy`](https://github.com/google/zerocopy) | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | Compiled in |
| [`zerocopy-derive`](https://github.com/google/zerocopy) | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | Compiled in |
| [`zerofrom`](https://github.com/unicode-org/icu4x) | 0.1.8 | Unicode-3.0 | Compiled in |
| [`zerofrom-derive`](https://github.com/unicode-org/icu4x) | 0.1.8 | Unicode-3.0 | Compiled in |
| [`zerotrie`](https://github.com/unicode-org/icu4x) | 0.2.5 | Unicode-3.0 | Compiled in |
| [`zerovec`](https://github.com/unicode-org/icu4x) | 0.11.9 | Unicode-3.0 | Compiled in |
| [`zerovec-derive`](https://github.com/unicode-org/icu4x) | 0.11.7 | Unicode-3.0 | Compiled in |
| [`zlib-rs`](https://github.com/trifectatechfoundation/zlib-rs) | 0.6.8 | Zlib | Compiled in |
| [`zmij`](https://github.com/dtolnay/zmij) | 1.0.23 | MIT | Compiled in |
| [`zune-core`](https://github.com/etemesi254/zune-image) | 0.5.3 | MIT OR Apache-2.0 OR Zlib | Compiled in |
| [`zune-jpeg`](https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg) | 0.5.15 | MIT OR Apache-2.0 OR Zlib | Compiled in |
