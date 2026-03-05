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
