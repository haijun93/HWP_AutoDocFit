# Outliner Engine API — 2026-11-21

`outliner_core::handle_request` accepts one JSON object and returns an envelope:

```json
{"id":"request-id","op":"parse","document":"- item\n","args":{}}
```

`id` is optional and is copied to the response (or `null`). `args` is optional. Document based operations read the top-level `document` string; a missing document is treated as an empty string. Successful responses use `{"id":…,"ok":true,"result":…}`. Errors use `{"id":…,"ok":false,"error":{"code":…,"message":…}}`.

## Operations

| `op` | Required `args` | Optional `args` / top-level fields | Successful `result` |
|---|---|---|---|
| `daily_page` | `date`: Gregorian `YYYY-MM-DD` | — | `filename`: canonical `YYYY_MM_DD.md` |
| `journal_name` | `date` or `name` | Accepts `YYYY_MM_DD`, `YYYY-MM-DD`, and either with `.md` (extension case-insensitive). A date input also returns adjacent journal names. | Name input: `date` (ISO) and canonical `name`; date input: `name`, `today`, `yesterday`, `tomorrow` |
| `journal_list` | `names`: string array | Names may use either separator and optional `.md`. Invalid calendar dates/names are ignored. | `names`: deduplicated canonical `YYYY_MM_DD` values, newest first |
| `query` | `query`: query expression | `pages`: array of `{page,document}`; otherwise optional `page`, with top-level `document` | `results`: matching page/block entries |
| `load` | `path`: UTF-8 Markdown file | — | `document` parsed model and original `markdown` |
| `save` | `path`: destination | `markdown` (otherwise top-level `document`); optional `expected_digest`: SHA-256 hex from the content read earlier | `path`, `saved: true`; a mismatch or missing destination with an expected digest returns `CONFLICT` without saving |
| `parse` | — | top-level `document` | Parsed document model |
| `serialize` | — | top-level `document` | `markdown`: serialized document |
| `edit` | `action`, `block_id` | `text` for text-taking actions; `offset` for cursor split | Updated `markdown` and parsed `document` |
| `search` | `query` | top-level `document` | `block_ids` |
| `backlinks` | `target` | top-level `document` | `block_ids` |
| `resolve_block_ref`, `block_ref` | One of `uuid`, `id`, `block_id`, `target` | `path` to load a file, or top-level `document` | Resolution object including `found`, block data, and backlinks |
| `rename_page`, `update_links` | `old_name`, `new_name` | `preview` or `dry_run`; either `files` array, `path`/`directory`/`dir`, or top-level `document` | Rename summary with `old_name`, `new_name`, affected files/count, and preview state; document mode also returns updated `markdown` |

Unknown operations return `INVALID_OPERATION`; malformed input objects return `INVALID_REQUEST`. An error from storage, parsing, or an edit is returned using its stable engine error code (including `CONFLICT`).

## `edit` actions

| `args.action` | Additional arguments | Behavior |
|---|---|---|
| `insert` | `text` | Inserts a new block after `block_id`, at the same indent |
| `delete` | — | Deletes the selected block |
| `indent`, `outdent` | — | Changes indent by one space; invalid boundary operations fail |
| `fold`, `unfold` | — | Changes the session fold flag |
| `up`, `down` | — | Swaps with an adjacent block |
| `split` | Preferred: integer `offset`; legacy: `text` separator if `offset` is absent | Splits at a grapheme-cluster offset. Offsets are clamped to the start/end. Korean syllables, combining sequences, and emoji graphemes remain intact. Legacy separator splitting remains available. |
| `merge` | — | Joins the next block, inserting one space when both texts are nonempty |
| `set_status` | `text`: `TODO`, `DONE`, or empty string | Sets or clears status |
| `toggle_status` | — | Cycles `None → TODO → DONE → TODO` |
| `set_text` | `text` | Replaces selected block text |

Split behavior is intentionally explicit: the original block ID remains on the front block and the new block receives a fresh ID. Indent, folded state, and TODO status are copied to the new block. Tags, properties, page links, and block references are recalculated from each resulting text fragment.

### Request shape warning

Do not send `op: "set_text"` directly; this is an unsupported operation and returns an error. Send `op: "edit"` with `args.action: "set_text"`, `args.block_id`, and `args.text`. The same wrapper applies to every edit action.

## Page-link rename rules

- Exact page-target matches are updated in `[[page]]` and `[[page|alias]]`; alias text is retained.
- Matching is case-sensitive. A target with different casing is left unchanged.
- `#tags`, nested wikilinks, inline code, and fenced code blocks are left unchanged.
- Preview and application use the same transformation. File preview reports affected paths/count without writing files; document mode returns the would-be updated Markdown.

## Saving with conflict detection

`content_digest(content)` computes SHA-256 over the exact UTF-8 bytes and returns 64 lowercase hexadecimal characters. `is_stale(read_digest, current)` is true when the digest differs. `save_atomic_checked(path, content, expected_digest)` checks current on-disk content before the atomic replacement. `save_atomic` remains available and keeps its previous unchecked behavior.

`sha2` is used because another crate in this repository already depends on it. `unicode-segmentation` supplies Unicode grapheme boundaries for cursor splitting; counting Rust `char`s would split combining sequences and joined emoji. No Windows APIs are used by `outliner_core`.
