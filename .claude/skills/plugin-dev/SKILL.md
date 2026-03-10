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

**`visible = bind(...)` does NOT work on container widgets:**

- `row` and `column` ignore dynamic `visible` binding — child widgets remain
  visible even when the bound property is `false`.
- Apply `visible = bind(...)` directly on each **leaf widget** (`static_text`,
  `edit_field`, `popup_menu`, `radio_button`, etc.) instead of the parent container.
- Alternative: use `enabled = bind(...)` on leaf widgets to gray them out
  instead of hiding. This avoids empty space and provides clearer UX when
  toggling between form sections (e.g. new task vs existing task).

**`static_text` does not support newlines:**

- `\n` inside a `static_text` title does not produce a line break.
- Split multi-line content into separate `static_text` widgets.

**`actionBinding` in `presentModalDialog` needs explicit `bind_to_object`:**

- `LrView.bind 'key'` does NOT work inside `actionBinding` because
  `presentModalDialog` has no `bind_to_object` context.
- Use the explicit form:
  ```lua
  actionBinding = {
      enabled = {
          bind_to_object = props,
          key = 'confirmEnabled',
      },
  },
  ```

**Escape sequences in `TranslatedStrings_*.txt`:**

- Translation files are plain text, NOT Lua source — Lua decimal escapes
  (`\ddd`) are printed literally, not interpreted as bytes.
- Write UTF-8 characters directly in the `.txt` file (e.g. `più`, not
  `pi\195\185`). Escape sequences only work in `.lua` string literals.

**`static_text` supports `enabled` for visual dimming:**

- Setting `enabled = false` (or binding) on `static_text` grays out the text,
  matching the native look of disabled fields. Useful for labels next to
  disabled `edit_field` or `popup_menu` widgets.

**Right-aligning elements in a row:**

- Use `place_horizontal = 1` on the widget to push it to the right edge.
- Or set `fill_horizontal = 1` on the preceding widget to absorb remaining space.

**`group_box` for visual grouping:**

- `group_box` draws a native border with a label in the top-left corner (like
  HTML `<fieldset>`). Accepts `title`, `fill_horizontal`, `spacing` and child
  widgets laid out vertically (like `column`).
- Replaces the pattern of `separator` + bold `static_text` title with a single
  semantic container — cleaner layout and less code.
- Works well for logically distinct sections inside a dialog (e.g. task detail,
  jobs list).

**`radio_button` grouping across platforms:**

- On macOS, radio buttons in the same container hierarchy are treated as a single
  native group — clicking one deselects all others, even if they bind to different
  properties. The Lua binding state stays correct, but the visual selection is wrong.
- Fix: place each logical radio group inside its own `group_box`. The `group_box`
  boundary acts as a native radio group separator on all platforms.

**System font constants for `font` property:**

- `'<system>'` — default system font.
- `'<system/bold>'` — bold variant, useful for section headings / sub-titles.
- `'<system/small>'` — smaller variant, useful for secondary text and inline
  warnings.
- `'<system/small/bold>'` — small + bold.
- These work on `static_text`, `edit_field`, `push_button`, and other widgets.

---

## Lua 5.1 Runtime Constraints

**`pcall` is incompatible with async SDK calls:**

- Lua 5.1 `pcall` is implemented in C and does **not** allow coroutine yields
  through its stack frame.
- Any SDK function that performs I/O (`LrHttp.get`, `LrFileUtils.exists`,
  `LrDialogs.presentModalDialog`, etc.) will crash with:
  `"Yielding is not allowed within a C or metamethod call"`
  if called inside `pcall`.
- This is fixed in Lua 5.2+ (coroutine-friendly pcall), but Lightroom is
  locked to 5.1.
- **Workaround**: use explicit cleanup (`running = false` at each return point)
  instead of try/finally patterns. For reentrancy guards, use a module-level
  `local running = false` flag with reset at every exit path.

**`addObserver` callbacks cannot call async SDK functions:**

- Observer callbacks (`props:addObserver`) run in a C stack frame — same yield
  restriction as `pcall`.
- Calling `LrHttp.get`, `LrHttp.post`, or any I/O SDK function inside an
  observer crashes with:
  `"Yielding is not allowed within a C or metamethod call (inside the callback for addObserver for condition <key>)"`
- **Workaround**: prefetch all needed data **before** opening the dialog
  (while still in the free async task context) and store it in a Lua table.
  The observer reads from the table instead of making network calls.
- This applies to all observer callbacks, not just `selectedTask` — any
  `props:addObserver(key, fn)` where `fn` calls an async SDK function will fail.

**`LrLibraryMenuItems` cannot be dynamically enabled/disabled:**

- `Info.lua` is a static table evaluated once at plugin load.
- `enabledWhen` only accepts predefined SDK values (e.g. `"photosAvailable"`),
  not custom Lua expressions or bound properties.
- To prevent concurrent execution from menu items, use `Guard.lua` — a shared
  module loaded via `require` (cached by Lua). Call `Guard.acquire('name')` at
  the start and `Guard.release('name')` at every exit point.
- **Do NOT use `local` flags** in menu item scripts — the file is re-executed
  fresh each time (`dofile`-style), so locals are re-initialized on every click.

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
- Sync: `ServerConnection.info(host)` — returns `(success, data)`, must be called from async context
- Async with callback: `ServerConnection.infoAsync(host, callback)`
