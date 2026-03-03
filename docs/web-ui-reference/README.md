# Web UI Reference

This directory preserves the original Lightroom plugin UI prototype (HTML/JS) and its
specification document. These files serve as **design inspiration** for a potential
web-based server management interface.

The prototype was originally designed without Lightroom SDK constraints in mind,
featuring rich interactive elements (dynamic lists, progress bars, color-coded pills,
modals) that are fully achievable in a web context but only partially reproducible
within the Lightroom plugin SDK.

## Files

- `PLUGIN_UI_PROTOTYPE.html` — Interactive HTML/JS prototype of the full plugin UI
- `PLUGIN_UI.md` — Detailed specification document (windows, modals, design rationale)

## Context

The Lightroom plugin implementation adapts these designs to work within the SDK's
limitations (see `plugin/CLAUDE.md` for the constraint catalog). A web-based interface
for the Photometoria server could implement the original vision without those
restrictions.
