# Photometoria Plugin — AI Assistant Context

## Plugin Structure

```
plugin/
├── photometoria.lrdevplugin/     # Lightroom plugin bundle
│   ├── Info.lua                  # Plugin metadata (SDK version, identifier)
│   ├── JSON.lua                  # JSON encoder/decoder (pure Lua)
│   ├── ServerConnection.lua      # HTTP client for Photometoria API
│   ├── PluginInfoProvider.lua    # Plugin Manager UI (connection settings)
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
- `LrLogger` — Debug logging

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
