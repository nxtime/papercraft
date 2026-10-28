# Incremental checks

Check what you just changed between edits, and check everything once at the end. This keeps the loop fast and the context small without skipping the final full pass.

## Why

* A full `lint_design` on a big page walks every layer and returns every finding. Run after each edit, it repeats the same output and burns tokens.
* Looping `get_node_info` over nodes is slower still: one measured loop over 397 nodes took about 53 s. One `get_bounds` call returns the same geometry for a whole subtree in milliseconds, in one result.
* Reads never change the document, so they never change `rev`. Only writes do.

## The flow

1. **Note `rev`.** Every write tool result includes `rev`, the document revision after the call. `get_basic_info` reports the current one. Keep the last value.
2. **Edit.** Make the change (`write_html`, `update_styles`, `set_text_content`, ...). Note the new `rev` from the result.
3. **Lint what changed.** `lint_design {"changedOnly": true}` checks since this client's last lint. `lint_design {"since": <rev>}` checks since a revision you name. A scoped lint covers the touched layers plus their ancestors, descendants and siblings, and every layer bound to a changed variable. The result says how much it covered, for example `{"checked": 12, "total": 397, "since": 40, "partial": true}`.
4. **See what an edit really touched.** `get_changes {"since": <rev>}` returns `{rev, since, truncated, changes: [{page, ids}]}`. Add `page` to limit it to one page. If `truncated` is `true`, the log no longer reaches that far back: do a full pass instead of trusting the list.
5. **Measure in one call.** `get_bounds {"nodeIds": [...]}` (or `pageId`, with `depth`) returns `columns` `[id, parent, x, y, w, h, absX, absY, clip]` and `rows`, one array per layer, parents first. If `truncated` is `true`, call it again with `continueWith` as `nodeIds` (same `depth`). Never loop `get_node_info` to read sizes or positions.
6. **Finish with the full check.** Before you report, always:
   * run `lint_design` with no arguments (a full pass, `partial: false`) and fix every error and warning, including `overflow`;
   * take a `get_screenshot` of every artboard you touched in each theme mode (`mode`, scale 2).

A partial lint is a speed-up, not a verdict. Only the final full pass can say the design is clean.

## Example

```json
get_basic_info {}
// {"rev": 40, ...}

write_html {"targetNodeId": "a1:1", "mode": "insert-children", "html": "<div class='Card'>...</div>"}
// {"rev": 41, ...}

lint_design {"changedOnly": true}
// {"errors": 0, "warnings": 1, "covered": {"checked": 12, "total": 397, "since": 40, "partial": true}, ...}

update_styles {"nodeIds": ["a1:7"], "styles": {"width": "320px"}}
// {"rev": 42, ...}

get_changes {"since": 41}
// {"rev": 42, "since": 41, "truncated": false, "changes": [{"page": "p1", "ids": ["a1:7"]}]}

get_bounds {"nodeIds": ["a1:1"], "depth": 3}
// {"columns": ["id", "parent", "x", "y", "w", "h", "absX", "absY", "clip"],
//  "rows": [["a1:1", null, 0, 0, 390, 844, 0, 0, true], ...]}

lint_design {}
// full pass, "partial": false, zero errors and warnings
get_screenshot {"nodeId": "a1:1", "scale": 2, "mode": "light"}
get_screenshot {"nodeId": "a1:1", "scale": 2, "mode": "dark"}
```
