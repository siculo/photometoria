# Project: Automatic Tagging System for Lightroom

## Objective
Develop an automatic photo tagging system for Adobe Lightroom using local AI models (Ollama) to analyze images and extract relevant tags.

## Project Context

### Hardware Requirements
- **OS:** Linux or macOS (Windows with WSL2 should also work)
- **GPU:** NVIDIA GPU with at least 8GB VRAM recommended for optimal performance
  - Models can also run on CPU, but inference will be significantly slower
- **Software:** Ollama installed and running (`ollama serve`)

### Configured AI Models

**Available models (tested and configured):**

1. **qwen3-vl:8b** (6.1 GB) → **PRODUCTION**
   - Maximum quality
   - Identifies specific landmarks (Eiffel Tower, Mont Saint-Michel, Normandy)
   - Always detailed and useful tags
   - Speed: ~2 min/photo
   - Temperature: 0.3 (deterministic)
   - **Use:** Real batches, maximum quality

2. **llava:latest** (4.7 GB) → **DEVELOPMENT**
   - Good quality/speed tradeoff
   - Speed: ~1 min/photo (2x faster)
   - Good but occasionally inconsistent tags
   - Temperature: 0.5
   - **Use:** Rapid iteration during development, logic/workflow testing

**Tested and discarded models:**
- ❌ **moondream** - Too lightweight, generates useless tags (only good descriptions)
- ❌ **llama3.2:latest** - Does not support images (text only)

### Commands to manage models:
```bash
ollama list                    # List installed models
ollama pull qwen3-vl:8b        # Download model
ollama rm moondream:latest     # Remove model
```

## Multi-Level Approach

The system supports different tag generation modes:

### 1. Single Photo Analysis (Fine Details)
- Specific tags for subjects, composition, colors
- Technical information from EXIF (camera, lens, settings)
- Configurable detail level (low/medium/high)

### 2. Group Analysis (Macro Categories)
- Temporal/spatial clustering of photos
- Common patterns among related photos
- Shared event/location tags

### 3. User Context Hints
- Ability to provide hints: location, event, period
- Example: "Vacation in Barcelona, summer 2024, Gaudí architecture"

### 4. EXIF Metadata
- GPS → automatic locations
- Date/Time → temporal events
- Camera/lens → technical info

### 5. Existing Tags Management
Configurable options:
- `merge`: combine with existing tags
- `ignore`: consider existing tags for context but don't modify them
- `replace`: completely replace

## Proposed Workflow

```
1. PREPARATION
   ├─ Select photo batch (manual operation on large batches)
   ├─ Automatic temporal clustering (via metadata)
   └─ User provides optional context hints

2. METADATA EXTRACTION
   ├─ GPS → locations
   ├─ Date/Time → events
   └─ Camera/lens → technical info

3. MULTI-LEVEL ANALYSIS
   ├─ Fine Level (per single photo)
   │  ├─ Specific subjects
   │  ├─ Composition
   │  └─ Dominant colors
   │
   └─ Context Level (per group)
      ├─ Common patterns
      ├─ Location (from GPS + AI)
      └─ Event type

4. INTELLIGENT MERGE
   ├─ Deduplicate similar tags
   ├─ Hierarchy (general vs specific)
   └─ Existing tags management

5. APPLY TO LIGHTROOM
   └─ Write keywords via Lua plugin (to be implemented)
```

## Current Project Status

### Current Phase: TESTING COMPLETED ✅

**Test results:**
- ✅ Ollama HTTP API working
- ✅ Qwen3-VL generates excellent tags (landmarks, colors, details)
- ✅ LLaVA usable for rapid development
- ✅ Multi-model support implemented
- ✅ Optimized prompts for each model

#### Created Files
- `test_models.py`: Complete Python script for AI testing with multi-model support
- `CLAUDE.md`: This project summary document

#### Directory Structure
```
scripts/
├── test_models.py               # Test script
├── test_images/                  # 12 test JPG photos
│   └── [France vacation photos] # Eiffel Tower, Mont Saint-Michel, cathedrals, etc.
└── test_results/                 # Test output (JSON)
    ├── test_results_qwen3-vl_8b_TIMESTAMP.json
    └── test_results_llava_latest_TIMESTAMP.json
```

#### Script Usage

**Basic commands:**
```bash
cd scripts

# Use default model (Qwen3-VL)
python3 test_models.py

# Use specific model
python3 test_models.py --model qwen3-vl:8b   # Maximum quality
python3 test_models.py --model llava          # Rapid development

# Compare all models
python3 test_models.py --compare

# List available models
python3 test_models.py --list
```

**Usage examples during development:**
```bash
# Development: rapid logic/workflow testing
python3 test_models.py --model llava

# Production: real batches with maximum quality
python3 test_models.py --model qwen3-vl:8b

# Verification: compare results
python3 test_models.py --compare
```

#### Tests Implemented in the Script

**TEST 1: Detailed Analysis (3 sample photos)**
- Complete image description
- Generic tags (subjects, objects, colors, composition, mood, location type)
- Detailed tags (secondary elements, lighting, time of day, weather, architectural style, activities, emotions)

**TEST 2: Quick Analysis (all 14 photos)**
- Fast tag generation for each photo
- Complete EXIF metadata extraction

**TEST 3: Group Analysis (first set of 7 photos)**
- Individual quick analysis
- Common pattern identification
- Contextual tags without user hints

**TEST 4: Group Analysis with Context Hints (second set of 7 photos)**
- Same as Test 3 but with user hints
- Testing context hints effectiveness
- Interactive input: "Summer 2022 vacation Lyon, Paris, Normandy and Brittany"

#### Output
JSON file with timestamp and model name in `scripts/test_results/` containing:
- EXIF metadata for each photo
- AI model responses for each test
- Tags generated with different strategies
- Model information used

#### Qualitative Results

**Test photos analyzed:**

1. **IMG_4465 (Eiffel Tower, Paris)**
   - Qwen3-VL: `woman, Eiffel Tower, car, backpack, tree, road, blue, black, white, red, central subject, open space, sunny, touristy, Paris landmark, plaza`
   - ✅ Identifies landmark, person, specific colors, mood

2. **IMG_4607 (Rouen Cathedral)**
   - Qwen3-VL: `Gothic architecture, vaulted ceiling, stained glass windows, ribbed vaults, cathedral interior, stone architecture, pointed arches, religious building, medieval architecture`
   - ✅ Precise architectural details

3. **IMG_4827 (Mont Saint-Michel)**
   - Qwen3-VL: `Mont Saint-Michel, tidal island, Normandy, France, medieval abbey, rocky island, wooden walkway, coastal, sea, beach, overcast sky, tourist attraction`
   - ✅ Identifies specific landmark and location (Normandy, France)

**Model comparison:**
- Qwen3-VL: Always detailed tags, identifies famous landmarks
- LLaVA: Good generic tags, occasionally generic placeholders ("subjects, objects")

## Technical Configuration

### Python Dependencies
- `Pillow`: for reading EXIF and handling images
- `requests`: for calling Ollama HTTP API
- Standard library: `json`, `pathlib`, `datetime`, `argparse`

### Dependencies Installation
```bash
pip install pillow requests --break-system-packages
```

### Ollama API
The script uses Ollama's HTTP API:
- Endpoint: `http://localhost:11434/api/chat`
- Method: POST with JSON payload
- Images: base64 encoded
- Format: Messages API with image support

### Model Configuration
Each model has:
- Specific optimized prompts (tags, detailed_tags, description, brief, group)
- Custom temperature
- Description and full name

### Current Limitations
- Ollama does not support multi-image in a single prompt
- For group analysis, photos are analyzed individually and then aggregated
- 120 second timeout for analysis
- Requires Ollama running: `ollama serve`

## Next Steps (to implement)

1. **Rust Version** (optional)
   - Higher speed for large batches (thousands of photos)
   - Single distributable binary
   - Native concurrency
   - Available libraries: kamadak-exif, image, reqwest, tokio

2. **Lua Plugin for Lightroom**
   - Direct integration with Lightroom
   - UI for photo selection and options
   - Write keywords to catalog
   - Existing tags management (merge/replace)

3. **Prompt Optimization**
   - Refine prompts based on real results
   - A/B testing on different prompt styles
   - Specialized prompts for photo type (portraits, landscapes, architecture)

4. **Advanced Features**
   - Intelligent clustering via GPS metadata
   - Location recognition via reverse geocoding
   - Automatic tag hierarchy
   - Export/import configurations
   - Batch processing with progress bar
   - Resume interrupted sessions

## Important Design Decisions

1. **Manual Operation on Batches**: not continuous processing, but manual operations on large photo batches
2. **Quality Priority**: chose Qwen3-VL:8b for maximum quality (we have sufficient VRAM)
3. **Flexibility**: configurable system with options for each analysis level
4. **JSON Output Format**: for easy parsing and future integration
5. **HTTP API vs CLI**: Use Ollama HTTP API instead of subprocess for greater reliability
6. **Multi-model**: Support for different models with optimized prompts for each

## Technical Notes

### Prompt Engineering
Each model responds differently to instructions:
- **Qwen3-VL**: Requires very directive and repeated instructions ("You must respond ONLY...")
- **LLaVA**: Prefers natural instructions with format examples
- **Anti-thinking prefix**: "Answer directly without showing your reasoning process" for Qwen

### EXIF Handling
- Complete metadata extraction (camera, lens, GPS, datetime)
- Separate GPS handling with GPSTags
- Bytes → string conversion for JSON compatibility
- File size calculation

### Performance
- Qwen3-VL: ~2 minutes per photo (detailed analysis)
- LLaVA: ~1 minute per photo (2x faster)
- Bottleneck: AI inference, not I/O or parsing

## Resources and References

**Ollama Documentation:**
- API: https://github.com/ollama/ollama/blob/main/docs/api.md
- Models: https://ollama.com/library

**Vision models used:**
- Qwen3-VL: https://ollama.com/library/qwen3-vl
- LLaVA: https://ollama.com/library/llava

**Lightroom SDK Setup (for future plugin):**
- SDK: https://developer.adobe.com/lightroom/
- Lua Documentation: https://www.lua.org/docs.html

## Open Questions (for future development)

- How to best handle merging between AI tags and existing tags?
- How many tags per photo are optimal for usability in Lightroom?
- How to structure tag hierarchy (general → specific)?
- Is it worth implementing a Rust version for performance?
- What UI/UX for the Lightroom plugin?
