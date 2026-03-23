# Development Guide

## Development Environment

### Requirements

**Software:**

- Adobe Lightroom Classic installed
- Lua development environment

**Supported Systems:**

- **Mac** - Fully supported
- **Windows** - Fully supported

### Software Stack

**Core Technologies:**

- **Lua** - Lightroom plugin scripting language
- **Adobe Lightroom SDK** - Plugin development framework
- **Git/GitHub** - Version control

### Recommended Development Tools

**Code Editors:**

- **VS Code** - Versatile editor with extensive plugin ecosystem
- **ZeroBrane Studio** - Lightweight Lua IDE

**Lua Development:**

- **Lua Language Server** - Language support and IntelliSense
- **Lua Debug** - Debugging support for Lua scripts

**Git Clients:**

- **Command-line git** - Standard git CLI
- **GUI options** - GitKraken, SourceTree, or other clients based on preference

## Getting Started

### Prerequisites

- Adobe Lightroom Classic installed
- Lua development environment configured
- API server running (see [API Development Guide](../../api/docs/development.md))

### Initial Setup

1. **Clone repository**

```bash
git clone https://github.com/yourusername/photometoria.git
cd photometoria/plugin
```

2. **Install the plugin in Lightroom**

- Open Lightroom Classic
- Go to **File > Plug-in Manager**
- Click **Add** and navigate to the plugin directory
- Enable the plugin

3. **Configure the plugin**

- Set the API server URL (default: `http://localhost:8080`)
- Configure authentication if required

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
    └── development.md            # This file
```

## Testing

### Unit Tests

Tests run outside Lightroom using a standalone Lua 5.1 interpreter:

```bash
# Run all plugin tests
lua plugin/tests/test_json.lua
lua plugin/tests/test_photo_validator.lua
lua plugin/tests/test_catalog_identity.lua
```

- `testkit.lua` provides `assertEqual`, `assertNil`, `assertNotNil`, `assertTableLength`
- Tests must not depend on Lightroom SDK modules (`import` is not available)
- Test files follow the pattern `test_<module>.lua`
- Modules with no SDK dependencies (`UUID.lua`, `JSON.lua`, `PhotoValidator.lua`)
  can be tested directly; SDK-dependent modules need mock stubs

### Manual Testing

- Test plugin loading in Lightroom Plug-in Manager
- Verify connection to API server
- Test photo upload and task management workflows
- Validate error handling for network issues
- Test on both Mac and Windows

### Debugging

**Enable Lightroom logging:**

- Use `LrLogger` for debug output
- Check Lightroom logs for plugin errors
- Use print statements during development

## Key Learnings

### Lightroom SDK

- Lightroom uses a sandboxed Lua environment
- Asynchronous operations require `LrTasks`
- HTTP requests use `LrHttp` module
- File access is restricted to specific directories

### Plugin Development

- Test with both Mac and Windows installations
- Handle network timeouts gracefully
- Provide clear user feedback for long operations
- Cache API responses when appropriate

## Future Roadmap

### Short-term

- Basic photo upload functionality
- Task creation from Lightroom
- Connection status indicator

### Medium-term

- Metadata write-back to Lightroom catalog
- UI for job monitoring and retry
- Batch processing support
- Progress indicators for uploads

### Long-term

- Offline queue for photos
- Smart collection integration
- Preset management
- Multi-catalog support

## See Also

- [API Development Guide](../../api/docs/development.md) - API server development
- [API Reference](../../api/docs/api-reference.md) - Complete endpoint documentation
