#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Photometoria contributors

"""
Translate tags from test_json_output.py into a target language using Ollama.
Reads JSON from stdin, outputs translated JSON to stdout.

Collects all unique tags, translates them in a single batch call,
then rebuilds the per-image JSON using the translation map.
"""

import argparse
import json
import re
import sys
import time

import requests

OLLAMA_URL = "http://localhost:11434/api/chat"

DEFAULT_MODEL = "translategemma:latest"
DEFAULT_BATCH_SIZE = 25

TRANSLATION_PROMPT = """\
Translate the following comma-separated list of photo tags from {source_language} into {language}.
Return ONLY the translated tags as a comma-separated list, in the same order.

{tags}\
"""


def call_ollama(model: str, prompt: str) -> str:
    payload = {
        "model": model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "stream": False,
        "options": {"temperature": 0.3},
    }

    resp = requests.post(OLLAMA_URL, json=payload, timeout=300)
    resp.raise_for_status()
    return resp.json()["message"]["content"].strip()


def parse_translated_list(raw: str, expected_count: int) -> list[str]:
    """Split comma-separated translated tags and validate count."""
    translated = [tag.strip() for tag in re.split(r"[,،、]", raw) if tag.strip()]

    if len(translated) == 0:
        raise ValueError("Empty translation response")
    if len(translated) != expected_count:
        raise ValueError(f"tag count mismatch: expected {expected_count}, got {len(translated)}")

    return translated


def build_translation_map(
    unique_tags: list[str], model: str, source_language: str, language: str,
    batch_size: int = DEFAULT_BATCH_SIZE,
) -> dict[str, str]:
    """Translate unique tags in batches and return a mapping."""
    translation_map = {}
    batches = [unique_tags[i:i + batch_size] for i in range(0, len(unique_tags), batch_size)]

    for batch_num, batch in enumerate(batches, 1):
        print(
            f"  Batch {batch_num}/{len(batches)} ({len(batch)} tags)...",
            file=sys.stderr,
        )
        tags_csv = ", ".join(batch)
        prompt = TRANSLATION_PROMPT.format(
            source_language=source_language, language=language, tags=tags_csv
        )

        raw = call_ollama(model, prompt)
        translated = parse_translated_list(raw, expected_count=len(batch))
        translation_map.update(zip(batch, translated))

    return translation_map


def main():
    parser = argparse.ArgumentParser(
        description="Translate tags from test_json_output.py into a target language",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""\
examples:
  cat tags.json | %(prog)s -l Italian
  cat tags.json | %(prog)s -l Italian -m qwen3.5:latest
  ./test_json_output.py llava | %(prog)s -l French
""",
    )
    parser.add_argument(
        "-m", "--model",
        default=DEFAULT_MODEL,
        help=f"Ollama model for translation (default: {DEFAULT_MODEL})",
    )
    parser.add_argument(
        "-l", "--language",
        required=True,
        help="Target language for translation",
    )
    parser.add_argument(
        "-b", "--batch-size",
        type=int,
        default=DEFAULT_BATCH_SIZE,
        help=f"Max tags per translation call (default: {DEFAULT_BATCH_SIZE})",
    )
    args = parser.parse_args()

    input_data = json.load(sys.stdin)

    source_language = input_data.get("language", "English")
    source_responses = input_data.get("responses", [])

    unique_tags = []
    seen = set()
    for entry in source_responses:
        if "validResponse" not in entry:
            continue
        for t in entry["validResponse"]["tags"]:
            tag = t["tag"]
            if tag not in seen:
                seen.add(tag)
                unique_tags.append(tag)

    print(
        f"Translating {len(unique_tags)} unique tags "
        f"(from {sum(len(e['validResponse']['tags']) for e in source_responses if 'validResponse' in e)} total)...",
        file=sys.stderr,
    )

    translation_failed = False
    error_description = None
    translation_map = {}

    start = time.monotonic()
    try:
        translation_map = build_translation_map(
            unique_tags, args.model, source_language, args.language,
            batch_size=args.batch_size,
        )
    except ValueError as e:
        translation_failed = True
        error_description = f"VALIDATION FAILED: {e}"
        print(f"Translation failed: {e}", file=sys.stderr)
    elapsed = time.monotonic() - start

    responses = []
    valid_count = 0
    invalid_count = 0

    for entry in source_responses:
        if "validResponse" not in entry:
            responses.append(entry)
            invalid_count += 1
            continue

        image_name = entry["image"]

        if translation_failed:
            responses.append({
                "image": image_name,
                "invalidResponse": entry["validResponse"],
                "errorDescription": error_description,
            })
            invalid_count += 1
        else:
            translated_tags = [
                {"tag": translation_map[t["tag"]]}
                for t in entry["validResponse"]["tags"]
            ]
            responses.append({
                "image": image_name,
                "validResponse": {"tags": translated_tags},
            })
            valid_count += 1

    source_summary = input_data.get("extractionSummary", input_data.get("summary", {}))

    result = {
        "language": args.language,
        "responses": responses,
        "extractionSummary": source_summary,
        "translationSummary": {
            "total": len(source_responses),
            "valid": valid_count,
            "invalid": invalid_count,
            "uniqueTags": len(unique_tags),
            "translationTimeInSeconds": f"{elapsed:.1f}",
        },
    }

    json.dump(result, sys.stdout, indent=4, ensure_ascii=False)
    print()


if __name__ == "__main__":
    main()
