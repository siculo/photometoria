# Photometoria Plugin — AI Assistant Context

## Plugin Structure

```
plugin/
├── photometoria.lrdevplugin/     # Lightroom plugin bundle
│   ├── Info.lua                  # Plugin metadata (SDK version, identifier)
│   ├── CatalogIdentity.lua      # Persistent catalog UUID (lazy-init via SDK prefs)
│   ├── UUID.lua                  # Pure-Lua UUID v4 generator (no SDK deps)
│   ├── JSON.lua                  # JSON encoder/decoder (pure Lua)
│   ├── Guard.lua                 # Reentrancy guard for menu items
│   ├── MockData.lua              # Mock data for UI development
│   ├── ServerConnection.lua      # HTTP client for Photometoria API
│   ├── PhotoValidator.lua        # Photo selection validation
│   ├── PhotoUploader.lua         # Batch photo upload with progress
│   ├── TaskUtils.lua             # Task-related utility functions
│   ├── PluginInfoProvider.lua    # Plugin Manager UI (connection settings)
│   ├── AddPhotosDialog.lua       # Add photos dialog (Library > Plugin Extras)
│   ├── ApplyTagsDialog.lua       # Apply tags dialog (Library > Plugin Extras)
│   ├── NewJobDialog.lua          # New job creation dialog
│   ├── TaskDialog.lua            # Task management dialog (Library > Plugin Extras)
│   ├── TaskDialogUI.lua          # Task dialog UI builder
│   └── TranslatedStrings_it.txt  # Italian localization
├── tests/                        # Unit tests (run outside Lightroom)
│   ├── testkit.lua               # Minimal test framework
│   ├── test_json.lua             # JSON module tests
│   ├── test_photo_validator.lua  # PhotoValidator tests
│   └── test_catalog_identity.lua # UUID generation tests
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
- `visible = bind(...)` on `row`/`column` does NOT hide children; apply on leaf widgets
- `\n` in `static_text` does not produce line breaks; use separate widgets
- `radio_button` groups in same container hierarchy merge on macOS; use `group_box` to isolate groups
- `actionBinding` in `presentModalDialog` needs explicit `bind_to_object = props`
- Lua escape sequences in `TranslatedStrings_*.txt` are NOT interpreted; write UTF-8 directly

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
function ServerConnection.info(host)
    local body, headers = LrHttp.get(url, nil, timeout)
    return success, data
end

-- Async wrapper: for callers that need fire-and-forget with callback
function ServerConnection.infoAsync(host, callback)
    LrTasks.startAsyncTask(function()
        local success, data = ServerConnection.info(host)
        callback(success, data)
    end)
end
```

### Catalog Identity

Each Lightroom catalog is assigned a persistent UUID, used to scope API calls
to the correct catalog on the server. The identity is split into two modules:

- **`UUID.lua`** — Pure-Lua UUID v4 generator. No SDK dependencies, fully
  testable outside Lightroom.
- **`CatalogIdentity.lua`** — Lazy-initializes and persists the catalog UUID
  via `catalog:getPropertyForPlugin` / `catalog:setPropertyForPlugin`. Must be
  called from within an async task context.

Dialogs that make catalog-scoped API calls (`TaskDialog`, `AddPhotosDialog`)
obtain the `catalogId` once at the start of their async task and pass it to
`ServerConnection` functions.

### Server Communication

The plugin communicates with the Photometoria API server via `ServerConnection.lua`:

- **Global endpoints** (no `catalogId`): `info(host)`, `listProviders(host)`,
  `providerDetails(host, name)`
- **Catalog-scoped endpoints**: `createTask(host, catalogId, name, context)`,
  `listTasks(host, catalogId)`
- **Resource endpoints** (UUID-addressed, no `catalogId`): `deleteTask`,
  `updateTask`, `uploadPhotos`, `listTaskJobs`, `createJob`, etc.

Helper: `catalogBasePath(catalogId)` → `'/api/catalogs/' .. catalogId`

### Localization

**All user-visible text** in the UI must use `LOC "$$$/Key=Default"` for localization.
When adding or modifying UI strings:

1. Use `LOC "$$$/Photometoria/Section/Key=English default"` in the Lua source.
2. Add the corresponding Italian translation in `TranslatedStrings_it.txt`.
3. Never leave a UI-facing string as a bare Lua literal — always wrap it with LOC.
