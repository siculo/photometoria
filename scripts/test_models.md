# test_models.py — Test Summary

## Tests

### TEST 1 — Detailed analysis (first 3 photos)

For each photo, runs 4 different analyses:

- **generic_tags** — generic tags (comma-separated)
- **detailed_tags** — detailed tags (more categories)
- **brief_description** — one-sentence description
- **full_description** — detailed description (with thinking mode enabled)

### TEST 2 — Quick analysis (all photos)

Generic tags only for each image.

### TEST 3 — Group analysis without context (first 7 photos)

Analyzes each photo individually, then looks for common/contextual tags
across the group using the first photo as representative.

### TEST 4 — Group analysis with context (photos from 8 onward)

Same as TEST 3 but with a context hint provided interactively by the user
(e.g., "Vacation in Barcelona, summer 2024").

## Additional features

- Extracts **EXIF metadata** from every analyzed photo
- Saves all results as a **timestamped JSON** file in `test_results/`
- Prints a **summary** at the end
- Supports `--compare` to test all configured models sequentially
