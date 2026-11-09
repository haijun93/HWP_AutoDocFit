# HIER6P hierarchy engine integration and measurement

## Measurement

One pass over the first 20 files in sorted `tests/fixtures` order. Python calls `docfit_core.style_hierarchy.analyze_hierarchy`; DLL calls the Rust C ABI through `docfit_app.hierarchy_bridge.run_library`. Times include input conversion and DLL invocation. UTF-8 output.

| Fixture | Paragraphs | Python (s) | DLL (s) |
|---|---:|---:|---:|
| ai_answer_chatgpt.txt | 24 | 0.001273 | 0.002777 |
| ai_answer_claude.txt | 37 | 0.001984 | 0.007435 |
| ai_answer_gemini.txt | 29 | 0.002883 | 0.005193 |
| ai_answer_mapo_pots.txt | 72 | 0.005377 | 0.011529 |
| beomgov_markdown.txt | 17 | 0.000752 | 0.002090 |
| c23_continuation.hwpx | 5 | 0.000396 | 0.000720 |
| c23_continuation.txt | 5 | 0.000381 | 0.000667 |
| gemini_bench_reply.txt | 12 | 0.000421 | 0.002113 |
| hwp_shapes_group_sample.hwpx | 7 | 0.000299 | 0.000947 |
| hwp_shapes_sample.hwpx | 1 | 0.000136 | 0.000246 |
| hwp_shapes_text_sample.hwpx | 7 | 0.000300 | 0.000963 |
| label228_circled_black.hwpx | 12 | 0.000516 | 0.001244 |
| label228_circled_black.txt | 13 | 0.000521 | 0.001269 |
| timemachine_extract/sample.hwpx | 3 | 0.000236 | 0.000430 |
| timemachine_extract/sample.txt | 4 | 0.000229 | 0.000610 |
| timemachine_extract/sample_cp949.txt | 3 | 0.000341 | 0.000600 |
| timemachine_extract/sample_utf16le.txt | 3 | 0.000317 | 0.000944 |
| timemachine_extract/sample_utf8_bom.txt | 3 | 0.000420 | 0.000729 |
| timport_basic.hwpx | 5 | 0.000307 | 0.000504 |
| timport_borders.hwpx | 5 | 0.000312 | 0.000482 |
| **Total** | **267** | **0.017401** | **0.041494** |

## Verification

- `cargo build --manifest-path hierarchy/Cargo.toml`: passed.
- `cargo build --release --manifest-path hierarchy/Cargo.toml`: passed; produced `hierarchy.dll` and CLI executable.
- `cargo test --manifest-path hierarchy/Cargo.toml`: 40 passed.
- Python hierarchy, page-fit wire, style hierarchy, time-machine content search, and document review modules: 90 passed, 4 platform skips.
- DLL corpus comparison: 29 documents, 549 paragraphs, 0 differences.