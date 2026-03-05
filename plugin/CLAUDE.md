# Photometoria Plugin — AI Assistant Context

## Plugin Structure

```
plugin/
├── photometoria.lrdevplugin/     # Lightroom plugin bundle
│   ├── Info.lua                  # Plugin metadata (SDK version, identifier)
│   ├── JSON.lua                  # JSON encoder/decoder (pure Lua)
│   ├── MockData.lua              # Mock data for UI development
│   ├── ServerConnection.lua      # HTTP client for Photometoria API
│   ├── PluginInfoProvider.lua    # Plugin Manager UI (connection settings)
│   ├── TaskDialog.lua            # Task management dialog (File > Plugin Extras)
│   └── TranslatedStrings_it.txt  # Italian localization
├── tests/                        # Unit tests (run outside Lightroom)
│   ├── testkit.lua               # Minimal test framework
│   └── test_json.lua             # JSON module tests
├── prototype/                    # UI/workflow prototypes
└── docs/                         # Plugin documentation
    └── development.md
```

---

## Lightroom SDK Constraints

- **Lua 5.1** runtime (Lightroom's embedded interpreter)
- **Sandbox**: no `os`, `io`, or `debug` libraries available inside Lightroom
- **Async model**: all network/UI operations must use `LrTasks.startAsyncTask()`
- **HTTP**: only `LrHttp.get()` and `LrHttp.post()` available (no PUT, PATCH, DELETE natively)
- **Localization**: `LOC "$$$/Key=Default"` pattern for translatable strings
- **No external dependencies**: everything must be pure Lua or SDK-provided

### Key SDK Modules

- `LrHttp` — HTTP requests
- `LrTasks` — Async task management
- `LrDialogs` — User dialogs and alerts
- `LrView` — UI construction (Plugin Manager sections)
- `LrBinding` — Property binding and observable tables
- `LrLogger` — Debug logging

### LrView Pitfalls (quick reference)

- `static_text` with empty initial value + `bind(...)` → stays zero-width; always
  set `fill_horizontal`, `width`, or `width_in_chars`
- `popup_menu` with `fill_horizontal` → button stretches but dropdown doesn't;
  use fixed `width`
- Unicode escapes inside LOC default values may not render; concatenate icons
  outside LOC: `'\226\156\147 ' .. LOC "$$$/Key=text"`

> **Full constraint catalog and development guidelines**: use `/plugin-dev` skill

---

## Code Conventions

### Async / Sync separation pattern

Functions that call `LrHttp` or other async-only SDK APIs must be split into two layers:

1. **Sync function** — performs the actual work, returns results directly. Must be
   called from within an existing `LrTasks.startAsyncTask` context.
2. **Async wrapper** — starts an async task, calls the sync function, and forwards
   results via callback.

This allows callers that already run inside an async task to use the sync version
directly, avoiding nested async tasks and callback-based synchronization.

Example (from `ServerConnection.lua`):

```lua
-- Sync: callable from any async task
function ServerConnection.fetch(host)
    local body, headers = LrHttp.get(url, nil, timeout)
    return success, data
end

-- Async wrapper: for callers that need fire-and-forget with callback
function ServerConnection.connect(host, callback)
    LrTasks.startAsyncTask(function()
        local success, data = ServerConnection.fetch(host)
        callback(success, data)
    end)
end
```

### Localization

**All user-visible text** in the UI must use `LOC "$$$/Key=Default"` for localization.
When adding or modifying UI strings:

1. Use `LOC "$$$/Photometoria/Section/Key=English default"` in the Lua source.
2. Add the corresponding Italian translation in `TranslatedStrings_it.txt`.
3. Never leave a UI-facing string as a bare Lua literal — always wrap it with LOC.
