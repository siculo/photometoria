#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Photometoria contributors

"""Debug: show full raw response for a single image."""

import base64
import json
import sys
from pathlib import Path

import requests

SCRIPT_DIR = Path(__file__).parent
PHOTO_DIR = SCRIPT_DIR / "test_images"
OLLAMA_URL = "http://localhost:11434/api/chat"

JSON_PROMPT = """\
Analyze this image and return descriptive tags as keywords.

Respond ONLY with valid JSON, no other text.
The JSON object must have a single key "tags" containing an array of objects.
Each object must have ONLY one key "tag" with a keyword string value.
Example: {"tags": [{"tag": "sunset"}, {"tag": "mountain"}, {"tag": "landscape"}, {"tag": "golden light"}, {"tag": "outdoor"}]}"""

def main():
    image_name = sys.argv[1] if len(sys.argv) > 1 else "IMG_4481.jpg"
    image_path = PHOTO_DIR / image_name

    with open(image_path, "rb") as f:
        image_b64 = base64.b64encode(f.read()).decode()

    payload = {
        "model": "llava:latest",
        "messages": [
            {"role": "user", "content": JSON_PROMPT, "images": [image_b64]}
        ],
        "stream": False,
        "options": {"temperature": 0.5},
    }

    resp = requests.post(OLLAMA_URL, json=payload, timeout=120)
    resp.raise_for_status()
    raw = resp.json()["message"]["content"].strip()

    print(f"Full raw response ({len(raw)} chars):")
    print(raw)
    print()

    # Try to parse
    try:
        data = json.loads(raw)
        print(f"Valid JSON: {json.dumps(data, indent=2)}")
    except json.JSONDecodeError as e:
        print(f"JSON parse error: {e}")
        # Show context around error position
        pos = e.pos
        print(f"Around position {pos}:")
        print(f"  ...{raw[max(0,pos-50):pos]}<<<HERE>>>{raw[pos:pos+50]}...")

if __name__ == "__main__":
    main()
