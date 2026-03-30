#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Photometoria contributors

"""
Test structured JSON tag prompt against Ollama models.
Outputs results as JSON to stdout for programmatic consumption.
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
    text = raw.strip()
    if text.startswith("```"):
        lines = text.splitlines()
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
        description="Test structured JSON tag prompt against Ollama models (JSON output)",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""\
examples:
  %(prog)s                                  Use default model and settings
  %(prog)s qwen3.5:latest                   Specify a model
  %(prog)s qwen3.5:0.8b -t 0.5             Custom temperature
  %(prog)s llava:latest -n 10               Test on 10 random images
  %(prog)s llava:latest -d /path/to/images  Use a different image directory
  %(prog)s llava:latest -p prompts/alt.txt  Use a different prompt file
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
        print(f"Image directory not found: {image_dir}", file=sys.stderr)
        sys.exit(1)

    prompt_path = args.prompt if args.prompt.is_absolute() else SCRIPT_DIR / args.prompt
    if not prompt_path.exists():
        print(f"Prompt file not found: {prompt_path}", file=sys.stderr)
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
        print(f"No images found in {image_dir}", file=sys.stderr)
        sys.exit(1)

    count = min(args.count, len(images))
    test_images = random.sample(images, count)

    responses = []
    valid_count = 0
    invalid_count = 0
    elapsed_times = []

    for i, img in enumerate(test_images, 1):
        print(f"Processing image {i}/{count}: {img.name}...", file=sys.stderr)
        start = time.monotonic()
        raw = call_ollama(img, model, temperature, prompt)
        elapsed = time.monotonic() - start
        elapsed_times.append(elapsed)

        try:
            data = validate_response(raw)
            responses.append({
                "image": img.name,
                "validResponse": data,
            })
            valid_count += 1
        except (json.JSONDecodeError, ValueError) as e:
            entry = {
                "image": img.name,
                "errorDescription": f"VALIDATION FAILED: {e}",
            }
            try:
                invalid_data = json.loads(raw)
                entry["invalidResponse"] = invalid_data
            except json.JSONDecodeError:
                entry["invalidResponse"] = raw
            responses.append(entry)
            invalid_count += 1

    avg_time = sum(elapsed_times) / len(elapsed_times) if elapsed_times else 0

    result = {
        "language": args.language,
        "responses": responses,
        "extractionSummary": {
            "total": count,
            "valid": valid_count,
            "invalid": invalid_count,
            "timePerPhotoInSeconds": f"{avg_time:.1f}",
        },
    }

    json.dump(result, sys.stdout, indent=4, ensure_ascii=False)
    print()


if __name__ == "__main__":
    main()
