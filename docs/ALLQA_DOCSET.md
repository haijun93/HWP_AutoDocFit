# ALLQA1 시험 문서 세트

개선 전 기준선 전용. 실제 사용자 업무 문서는 사용하지 않고 저장소 테스트 픽스처만 사용한다. 실행 시 원본 픽스처를 사용자가 지정한 외부 문서 폴더로 복사한다. 쪽 수는 COM 저장 이벤트에서 취득한다.

| ID | 종류/특성 | 원본(저장소 상대 경로) | SHA-256 |
|---|---|---|---|
| txt_short | 짧은 TXT, 연속 문장 | `tests/fixtures/c23_continuation.txt` | `25c41aef3dcefb192a37acbed46b2dc93288ab454ac92000a4ff3a07b12fc661` |
| txt_long | 긴 TXT, 모의 텍스트 | `tests/fixtures/ai_answer_mapo_pots.txt` | `b17ecc60ce07df0337e135970b0f1ed4a0e36a19a5e638db84196d91007eac22` |
| hwp_reference | HWP 픽스처 | `tests/fixtures/KIDX13_reference.hwp` | `bf6093c472178b718e43cdc0168fb516823cdc084f3c2d6845f42bc0454506b3` |
| hwpx_table | HWPX 표 포함 | `tests/fixtures/attp_table.hwpx` | `037d2ac7bffe9edb2a254c339a17af7b642db6a7638aa9e8e75b9f562942cd87` |
| hwpx_continuation | HWPX 긴 연속문서 | `tests/fixtures/c23_continuation.hwpx` | `48b2ce38834a052d9a4581f7b722e1432143860ce92a227534142df407a18237` |
| hwpx_shapes | HWPX 도형·텍스트 포함 | `tests/fixtures/hwp_shapes_text_sample.hwpx` | `21b032f9787fbd61d3acbcca6961a14433322b8c8b743201c903354370ecd65d` |

<!-- ALLQA-DATA -->
| id | source | hash | kind |
|---|---|---|---|
| txt_short | tests/fixtures/c23_continuation.txt | 25c41aef3dcefb192a37acbed46b2dc93288ab454ac92000a4ff3a07b12fc661 | TXT 짧은 연속문장 |
| txt_long | tests/fixtures/ai_answer_mapo_pots.txt | b17ecc60ce07df0337e135970b0f1ed4a0e36a19a5e638db84196d91007eac22 | TXT 긴 모의텍스트 |
| hwp_reference | tests/fixtures/KIDX13_reference.hwp | bf6093c472178b718e43cdc0168fb516823cdc084f3c2d6845f42bc0454506b3 | HWP |
| hwpx_table | tests/fixtures/attp_table.hwpx | 037d2ac7bffe9edb2a254c339a17af7b642db6a7638aa9e8e75b9f562942cd87 | HWPX 표 |
| hwpx_continuation | tests/fixtures/c23_continuation.hwpx | 48b2ce38834a052d9a4581f7b722e1432143860ce92a227534142df407a18237 | HWPX 연속문서 |
| hwpx_shapes | tests/fixtures/hwp_shapes_text_sample.hwpx | 21b032f9787fbd61d3acbcca6961a14433322b8c8b743201c903354370ecd65d | HWPX 도형·텍스트 |
<!-- /ALLQA-DATA -->

주의: 이 세트는 형식·구조 다양성을 위한 것이며, 특정 1쪽·5쪽·10쪽 경계 샘플이 확보됐다고 주장하지 않는다. 쪽 수를 얻지 못한 문서는 결과 보고서에 `UNMEASURED`로 남긴다.

실행 예: `python scripts/allqa_run.py --docset docs/ALLQA_DOCSET.md --out <외부 결과 폴더> --docs-dir <외부 문서 사본 폴더> --label before`
