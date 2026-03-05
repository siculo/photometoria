---
name: plugin-dev
description: Use this skill when working on the Lightroom plugin (`plugin/` directory).
---

# Photometoria Plugin — Development Guidelines

## LrView UI Constraints (for prototype revision)

The LrView SDK imposes significant constraints on what UI elements are possible.
These must be considered when designing or revising the plugin UI prototype.

**Available list widgets:**

| Widget | Content | Selection value | Notes |
|--------|---------|-----------------|-------|
| `popup_menu` | Text-only items (`title` + `value`) | Scalar (e.g. `1`) | Dropdown, compact |
| `simple_list` | Text-only items (`title` + `value`) | Table (e.g. `{1}`) | Scrollable, min height 80px |

Neither widget supports rich content (icons, colors, multi-line) per item.

**No dynamic view trees:**

- The view tree is **built once** and is static after construction.
- To simulate dynamic lists, use **pre-allocated slots** with `visible = bind(...)`.
  Each slot binds to properties that are updated when data changes; unused slots
  are hidden.
- Alternatively, use a single detail panel bound to the selected item's properties
  (master-detail pattern). Example: job detail panel in `TaskDialog.lua`.

**No click/mouse events on containers:**

- `row`, `column`, `scrolled_view` have **no** click or mouse event handlers.
- Only `push_button` has an `action` handler; `catalog_photo` has `mouse_down`.
- Custom selectable list items (e.g. radio buttons inside a scrolled_view) are
  possible but with limited UX: selection via radio dot, not full-row highlight.

**No background color on layout containers:**

- `row` and `column` are transparent — no `background_color` property.
- `scrolled_view` has `background_color` but min height is 80px and includes scrollbars.
- **Cannot create colored rectangles** for progress bars or status pills using containers.

**Progress bar workaround (ProgressBar component):**

A reusable progress bar is implemented in `TaskDialog.lua` using:
- A **disabled `edit_field`** (provides native border) filled with Unicode block
  characters (`█` = `\226\150\136`, space for empty) in monospace font.
- A **`static_text`** next to it showing the percentage label.
- Encapsulated as `ProgressBar.init()`, `ProgressBar.build()`, `ProgressBar.set()`,
  `ProgressBar.clear()` — callers interact through a single key string.

**Lua 5.1 string escapes:**

- `\xNN` hex escapes do **NOT** work in Lua 5.1 (introduced in 5.2).
- Use **decimal escapes** `\ddd` for raw bytes: e.g. `\194\183` for `·` (middle dot),
  `\226\150\136` for `█` (full block).
- Alternatively, write UTF-8 characters directly in the source file.

**Text color limitations:**

- `static_text.text_color` accepts `LrColor` but may not support dynamic binding
  on all platforms. Use text indicators (e.g. `[Active]`, `[Errors]`) as fallback
  for color-coded status.

**`static_text` with dynamic binding needs explicit width:**

- A `static_text` built with an empty initial value (`''`) and updated via
  `bind(...)` keeps its zero width from construction time — text appears truncated.
- Always set `fill_horizontal = 1`, `width`, or `width_in_chars` on any
  `static_text` whose value changes at runtime.

**`popup_menu` does not auto-size to content:**

- With `fill_horizontal = 1` the button stretches to fill the row, but the native
  dropdown menu appears at its own smaller width, causing visual misalignment.
- Use a fixed `width` calibrated to the expected content length.

**Unicode icons inside LOC strings may not render:**

- Decimal escape sequences (e.g. `\226\156\147`) inside a LOC default value
  (`"$$$/Key=\226\156\147 text"`) may not display correctly.
- Concatenate the icon **outside** LOC: `'\226\156\147 ' .. LOC "$$$/Key=text"`.

**Translation file caching:**

- `TranslatedStrings_<locale>.txt` overrides LOC defaults and is **not reloaded**
  by Plugin Manager's "Reload" — a full Lightroom restart is required.
- If a UI label appears stale after code changes, check for an outdated translation
  in the `.txt` file.

**Right-aligning elements in a row:**

- Use `place_horizontal = 1` on the widget to push it to the right edge.
- Or set `fill_horizontal = 1` on the preceding widget to absorb remaining space.

---

## Testing

Tests run **outside Lightroom** using a standalone Lua 5.1 interpreter:

```bash
# Run JSON tests
lua plugin/tests/test_json.lua
```

- `testkit.lua` provides `assert_equals`, `assert_true`, `assert_error`, `run_tests`
- Tests must not depend on Lightroom SDK modules (`import` is not available)
- Test files follow the pattern `test_<module>.lua`

---

## Development Workflow

1. **Edit** plugin files in `plugin/photometoria.lrdevplugin/`
2. **Test** with standalone Lua for testable logic (JSON, validation)
3. **Install** in Lightroom: Plugin Manager → Add → select `.lrdevplugin` folder
4. **Debug** with `LrLogger` — logs appear in Lightroom's plugin log directory
5. **Reload** plugin in Plugin Manager after changes (or restart Lightroom)

### Server Communication

The plugin communicates with the Photometoria API server via `ServerConnection.lua`:
- Connection endpoint: `GET /api/info` (server capabilities and status)
- Host format validation: `host:port` (e.g., `localhost:3000`)
- Async with callback pattern: `ServerConnection.connect(host, callback)`
