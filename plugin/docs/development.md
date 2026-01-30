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
├── Info.lua              # Plugin metadata and version
├── PluginInit.lua        # Plugin initialization
├── PluginManager.lua     # Plugin manager dialog
├── PhotometoriaAPI.lua   # API client module
├── ExportServiceProvider.lua  # Export service integration
└── docs/
    └── development.md    # This file
```

## Testing

### Manual Testing

- Test plugin loading in Lightroom Plug-in Manager
- Verify connection to API server
- Test photo export and metadata workflows
- Validate error handling for network issues

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
