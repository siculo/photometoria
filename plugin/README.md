# Photometoria Plugin

## Overview

This is the **Lightroom Classic plugin** component of Photometoria, implemented in Lua using the Adobe Lightroom SDK. It provides direct integration with Adobe Lightroom Classic, allowing photographers to send photos to the Photometoria API for AI-powered metadata generation and receive results directly in their catalog.

## Quick Start

### Prerequisites

- Adobe Lightroom Classic installed
- Photometoria API server running (see [API README](../api/README.md))
- macOS or Windows

### Installation

1. **Clone the repository:**
   ```bash
   git clone https://github.com/yourusername/photometoria.git
   ```

2. **Install the plugin in Lightroom:**
   - Open Lightroom Classic
   - Go to **File > Plug-in Manager**
   - Click **Add** and navigate to `photometoria/plugin`
   - Enable the plugin

3. **Configure the plugin:**
   - In Plug-in Manager, select Photometoria
   - Set the API server URL (default: `http://localhost:8080`)
   - Click **Done**

## Documentation

- **[Development Guide](docs/development.md)** - Setup, testing, and workflow

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for development guidelines.

## License

Apache 2.0 - See [LICENSE](../LICENSE) for details.

## Version

Current: v0.1.0 (Planned)

---

*For the main Photometoria project documentation, see the [root README](../README.md).*
