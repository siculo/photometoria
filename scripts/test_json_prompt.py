#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Photometoria contributors

"""
Quick test for structured JSON tag prompt against Ollama models.
Tests whether models return valid JSON in the expected format.
"""

import argparse
import base64
import json
import random
import sys
import time
from pathlib import Path

import requests

SCRIPT_DIR = Path(__file__).parent
PHOTO_DIR = SCRIPT_DIR / "test_images"

OLLAMA_URL = "http://localhost:11434/api/chat"

DEFAULT_PROMPT_FILE = SCRIPT_DIR / "prompts" / "default.txt"
DEFAULT_MODEL = "llava:latest"
DEFAULT_TEMPERATURE = 0.3
DEFAULT_LANGUAGE = "English"


def call_ollama(image_path: Path, model: str, temperature: float, prompt: str) -> str:
    with open(image_path, "rb") as f:
        image_b64 = base64.b64encode(f.read()).decode()

    payload = {
        "model": model,
        "messages": [
            {"role": "user", "content": prompt, "images": [image_b64]}
        ],
        "stream": False,
        "options": {"temperature": temperature},
    }

    resp = requests.post(OLLAMA_URL, json=payload, timeout=300)
    resp.raise_for_status()
    return resp.json()["message"]["content"].strip()


def validate_response(raw: str) -> dict:
    """Parse and validate against the expected schema."""
    # Some models wrap JSON in markdown code blocks
    text = raw.strip()
    if text.startswith("```"):
        lines = text.splitlines()
        # Remove first and last ``` lines
        lines = [l for l in lines if not l.strip().startswith("```")]
        text = "\n".join(lines).strip()

    data = json.loads(text)

    if "tags" not in data:
        raise ValueError("Missing 'tags' key")
    if not isinstance(data["tags"], list):
        raise ValueError("'tags' is not an array")
    if len(data["tags"]) == 0:
        raise ValueError("'tags' array is empty")

    for i, entry in enumerate(data["tags"]):
        if not isinstance(entry, dict):
            raise ValueError(f"tags[{i}] is not an object: {entry}")
        if "tag" not in entry:
            raise ValueError(f"tags[{i}] missing 'tag' key: {entry}")
        if not isinstance(entry["tag"], str) or not entry["tag"].strip():
            raise ValueError(f"tags[{i}].tag is empty or not a string")
        extra_keys = set(entry.keys()) - {"tag"}
        if extra_keys:
            raise ValueError(f"tags[{i}] has unexpected keys {extra_keys}: {entry}")

    return data


def main():
    parser = argparse.ArgumentParser(
        description="Test structured JSON tag prompt against Ollama models",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""\
examples:
  %(prog)s                                  Use default model and settings
  %(prog)s qwen3.5:latest                   Specify a model
  %(prog)s qwen3.5:0.8b -t 0.5             Custom temperature
  %(prog)s llava:latest -n 10               Test on 10 random images
  %(prog)s llava:latest -d /path/to/images  Use a different image directory
  %(prog)s llava:latest -p prompts/alt.txt  Use a different prompt file

files:
  prompts/default.txt           Default prompt template (supports {context} placeholder)
  <image-dir>/context.txt       Optional context injected into the prompt
""",
    )
    parser.add_argument(
        "model",
        nargs="?",
        default=DEFAULT_MODEL,
        help=f"Ollama model name (default: {DEFAULT_MODEL})",
    )
    parser.add_argument(
        "-t", "--temperature",
        type=float,
        default=DEFAULT_TEMPERATURE,
        help=f"Model temperature (default: {DEFAULT_TEMPERATURE})",
    )
    parser.add_argument(
        "-l", "--language",
        default=DEFAULT_LANGUAGE,
        help=f"Language for generated tags (default: {DEFAULT_LANGUAGE})",
    )
    parser.add_argument(
        "-n", "--count",
        type=int,
        default=4,
        help="Number of random images to test (default: 4)",
    )
    parser.add_argument(
        "-p", "--prompt",
        type=Path,
        default=DEFAULT_PROMPT_FILE,
        help=f"Path to prompt file (default: {DEFAULT_PROMPT_FILE.relative_to(SCRIPT_DIR)})",
    )
    parser.add_argument(
        "-d", "--image-dir",
        type=Path,
        default=PHOTO_DIR,
        help=f"Directory with test images (default: {PHOTO_DIR.relative_to(SCRIPT_DIR)})",
    )
    args = parser.parse_args()

    image_dir = args.image_dir if args.image_dir.is_absolute() else SCRIPT_DIR / args.image_dir
    if not image_dir.is_dir():
        print(f"Image directory not found: {image_dir}")
        sys.exit(1)

    prompt_path = args.prompt if args.prompt.is_absolute() else SCRIPT_DIR / args.prompt
    if not prompt_path.exists():
        print(f"Prompt file not found: {prompt_path}")
        sys.exit(1)
    prompt_template = prompt_path.read_text().strip()

    context_file = image_dir / "context.txt"
    context = ""
    if context_file.exists():
        context = f"Context: {context_file.read_text().strip()}\n\n"
    prompt = prompt_template.replace("{context}", context).replace("{language}", args.language)

    model = args.model
    temperature = args.temperature
    images = sorted(image_dir.glob("*.jpg")) + sorted(image_dir.glob("*.JPG"))

    if not images:
        print(f"No images found in {image_dir}")
        sys.exit(1)

    count = min(args.count, len(images))
    test_images = random.sample(images, count)
    print(f"Model: {model}")
    print(f"Temperature: {temperature}")
    print(f"Prompt: {prompt_path.relative_to(SCRIPT_DIR)}")
    print(f"Testing {count} random images out of {len(images)} available\n")
    print(f"Prompt:\n  {prompt}\n")

    passed = 0
    failed = 0
    elapsed_times = []

    for img in test_images:
        print(f"--- {img.name} ---")
        start = time.monotonic()
        raw = call_ollama(img, model, temperature, prompt)
        elapsed = time.monotonic() - start
        elapsed_times.append(elapsed)
        print(f"  Raw response:\n{raw}")
        print(f"  Time: {elapsed:.1f}s")

        try:
            data = validate_response(raw)
            tags = [e["tag"] for e in data["tags"]]
            print(f"  Valid JSON with {len(tags)} tags: {', '.join(tags)}")
            passed += 1
        except (json.JSONDecodeError, ValueError) as e:
            print(f"  VALIDATION FAILED: {e}")
            failed += 1

        print()

    avg_time = sum(elapsed_times) / len(elapsed_times)
    print(f"Results: {passed} passed, {failed} failed out of {len(test_images)}")
    print(f"Average time per photo: {avg_time:.1f}s")

    print("\n" + "=" * 60)
    print("Proposed config.toml entry")
    print("=" * 60)

    model_id = model.replace(":", "_").replace("-", "_").replace("/", "_")
    print(f"""
[ai.providers.ollama.models.{model_id}]
ollama_model = "{model}"
prompt_template = \"\"\"
{prompt_template}\"\"\"
description = ""
supports_vision = true
""")


if __name__ == "__main__":
    main()
