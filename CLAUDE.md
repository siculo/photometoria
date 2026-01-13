# Photometoria

## Project Objective

Photometoria is an integrated system for AI-powered photographic image analysis and automatic metadata generation (keywords, titles, descriptions).

The project consists of:

- **REST API**: Backend service that orchestrates image analysis using local AI models (Ollama)
- **Lightroom Plugin**: Lua extension for Adobe Lightroom Classic that allows photographers to send images to the API and receive generated metadata directly in their catalog
- **Support Scripts**: Python tools for testing and validating AI models

The main goal is to automate the photographic keywording process, drastically reducing the time required to catalog images while maintaining high quality and precision in the generated metadata.
