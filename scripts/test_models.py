#!/usr/bin/env python3
"""
Test script for tag generation with Qwen3-VL
Analyzes photos with different approaches and saves results
"""

import os
import json
from datetime import datetime
from pathlib import Path
from typing import List, Dict, Any
import base64
import argparse
import sys

try:
    from PIL import Image
    from PIL.ExifTags import TAGS, GPSTAGS
    import requests
except ImportError:
    print("Installing required dependencies...")
    import subprocess
    subprocess.run(["pip", "install", "pillow", "requests", "--break-system-packages"], check=True)
    from PIL import Image
    from PIL.ExifTags import TAGS, GPSTAGS
    import requests

# Configuration with relative paths
SCRIPT_DIR = Path(__file__).parent
PHOTO_DIR = SCRIPT_DIR / "test_images"
RESULTS_DIR = SCRIPT_DIR / "test_results"

# Available model configurations
MODEL_CONFIGS = {
    "qwen3-vl:8b": {
        "name": "qwen3-vl:8b",
        "description": "Maximum quality, slower",
        "temperature": 0.3,
        "prompts": {
            "tags": """You must respond ONLY with a comma-separated list of tags. Do not write sentences. Do not explain. Only output tags separated by commas.

Tags to include: subjects, objects, colors, composition, mood, location type.

Output format example: tag1, tag2, tag3, tag4

Now analyze this image and output ONLY the tags:""",
            "detailed_tags": """You must respond ONLY with a comma-separated list of detailed tags. Do not write sentences. Do not explain. Only output tags separated by commas.

Tags to include: main subjects, secondary elements, colors, lighting, time of day, weather, architectural style, activities, emotions.

Output format example: tag1, tag2, tag3, tag4, tag5, tag6

Now analyze this image and output ONLY the tags:""",
            "description": "Describe this image in detail. What do you see?",
            "brief": "Briefly describe what you see in this image (one sentence):",
            "group": "Output ONLY a comma-separated list of contextual tags for this photo collection. No descriptions, no sentences, only tags."
        }
    },
    "llava": {
        "name": "llava:latest",
        "description": "Good quality/speed tradeoff",
        "temperature": 0.5,
        "prompts": {
            "tags": """List relevant tags for this image as a comma-separated list.
Include: subjects, objects, colors, composition, mood, location.
Format: tag1, tag2, tag3""",
            "detailed_tags": """Provide detailed tags for this image as a comma-separated list.
Include: subjects, elements, colors, lighting, weather, architecture, activities.
Format: tag1, tag2, tag3, tag4""",
            "description": "Describe this image in detail.",
            "brief": "In one sentence, what do you see in this image?",
            "group": "Provide contextual tags for this collection of photos (comma-separated list)."
        }
    }
}

# Default model
DEFAULT_MODEL = "qwen3-vl:8b"

class PhotoAnalyzer:
    def __init__(self, photo_dir: str, results_dir: str, model_name: str = None):
        self.photo_dir = Path(photo_dir)
        self.results_dir = Path(results_dir)
        self.results_dir.mkdir(parents=True, exist_ok=True)
        
        # Configure model
        if model_name is None:
            model_name = DEFAULT_MODEL
        
        if model_name not in MODEL_CONFIGS:
            available = ", ".join(MODEL_CONFIGS.keys())
            raise ValueError(f"Model '{model_name}' not recognized. Available: {available}")
        
        self.model_config = MODEL_CONFIGS[model_name]
        self.model_name = self.model_config["name"]
        print(f"Using model: {self.model_name} - {self.model_config['description']}")
        
    def extract_exif(self, image_path: Path) -> Dict[str, Any]:
        """Extracts EXIF metadata from the photo"""
        metadata = {
            'filename': image_path.name,
            'path': str(image_path.relative_to(SCRIPT_DIR)),
            'file_size_mb': image_path.stat().st_size / (1024 * 1024),
        }
        
        try:
            image = Image.open(image_path)
            exif_data = image._getexif()
            
            if exif_data:
                for tag_id, value in exif_data.items():
                    tag = TAGS.get(tag_id, tag_id)
                    
                    # GPS handling
                    if tag == "GPSInfo":
                        gps_data = {}
                        for gps_tag_id in value:
                            gps_tag = GPSTAGS.get(gps_tag_id, gps_tag_id)
                            gps_data[gps_tag] = value[gps_tag_id]
                        metadata['gps'] = gps_data
                    else:
                        # Convert datetime
                        if isinstance(value, bytes):
                            value = value.decode(errors='ignore')
                        metadata[tag] = str(value) if not isinstance(value, (str, int, float)) else value
                        
            # Image dimensions
            metadata['dimensions'] = f"{image.width}x{image.height}"
            
        except Exception as e:
            metadata['exif_error'] = str(e)
            
        return metadata
    
    def call_ollama(self, image_path: Path, prompt_type: str, use_thinking: bool = False, custom_prompt: str = None) -> Dict[str, Any]:
        """Calls Ollama API to analyze the image
        
        Args:
            image_path: Image path
            prompt_type: Type of prompt to use ('tags', 'detailed_tags', 'description', 'brief', 'group')
            use_thinking: If True, allows the model to reason (only for descriptions)
            custom_prompt: Custom prompt (overrides prompt_type)
        """
        print(f"  Analyzing {image_path.name}...")
        
        try:
            # Read the image and convert to base64
            with open(image_path, 'rb') as img_file:
                image_data = base64.b64encode(img_file.read()).decode('utf-8')
            
            # Determine which prompt to use
            if custom_prompt:
                prompt = custom_prompt
            else:
                prompt = self.model_config["prompts"].get(prompt_type, "Describe this image.")
            
            # Prepare the API payload
            payload = {
                "model": self.model_name,
                "messages": [
                    {
                        "role": "user",
                        "content": prompt,
                        "images": [image_data]
                    }
                ],
                "stream": False,
                "options": {
                    "temperature": self.model_config["temperature"],
                }
            }
            
            # If we don't want thinking mode, specify it in the prompt (only for some models)
            if not use_thinking and "qwen" in self.model_name.lower():
                payload["messages"][0]["content"] = "Answer directly without showing your reasoning process. " + prompt
            
            # Call the API
            response = requests.post(
                'http://localhost:11434/api/chat',
                json=payload,
                timeout=120
            )
            
            if response.status_code != 200:
                return {
                    'success': False,
                    'error': f'HTTP {response.status_code}: {response.text}',
                    'prompt': prompt
                }
            
            # Parse the JSON response
            result = response.json()
            
            # Extract response content
            if 'message' in result and 'content' in result['message']:
                return {
                    'success': True,
                    'response': result['message']['content'].strip(),
                    'prompt': prompt
                }
            else:
                return {
                    'success': False,
                    'error': f'Unexpected response format: {result}',
                    'prompt': prompt
                }
            
        except requests.Timeout:
            return {
                'success': False,
                'error': 'Request timeout (>120s)',
                'prompt': custom_prompt or self.model_config["prompts"].get(prompt_type, "")
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e),
                'prompt': custom_prompt or self.model_config["prompts"].get(prompt_type, "")
            }
    
    def get_images(self) -> List[Path]:
        """Gets the list of images to process"""
        extensions = ['.jpg', '.jpeg', '.png', '.JPG', '.JPEG', '.PNG']
        images = []
        for ext in extensions:
            images.extend(self.photo_dir.glob(f'*{ext}'))
        
        return sorted(images)
    
    def analyze_photo_group(self, images: List[Path], context_hint: str = None) -> Dict[str, Any]:
        """Analyzes a group of photos to find common/contextual tags"""
        
        # First get individual tags for each photo
        individual_results = []
        for img in images:
            result = self.call_ollama(img, 'tags', use_thinking=False)
            individual_results.append({
                'image': str(img.relative_to(SCRIPT_DIR)),
                'tags': result
            })
        
        # Then analyze the group as a whole
        # Prepare a prompt with context hint if provided
        if context_hint:
            group_prompt = f"""Context: {context_hint}

Based on this context and the images provided, output ONLY a comma-separated list of contextual tags that apply to this photo collection. No descriptions, no sentences, only tags."""
        else:
            group_prompt = self.model_config["prompts"]["group"]
        
        # For group analysis, use the first image as representative
        # (in a more advanced system, multiple images could be sent)
        group_result = self.call_ollama(images[0], 'group', custom_prompt=group_prompt)
        
        return {
            'context_hint': context_hint,
            'individual_analyses': individual_results,
            'group_analysis': group_result,
            'images_count': len(images)
        }
    
    def run_comprehensive_test(self):
        """Runs a comprehensive series of tests on the system"""
        
        # Get image list
        images = self.get_images()
        
        if not images:
            print(f"\nNo images found in {self.photo_dir}")
            return None
        
        print(f"\nFound {len(images)} images to analyze")
        
        # Prepare results structure
        all_results = {
            'timestamp': datetime.now().isoformat(),
            'model': self.model_name,
            'model_config': self.model_config['description'],
            'total_images': len(images),
            'tests': {}
        }
        
        # TEST 1: Detailed analysis on sample photos
        print("\n" + "="*60)
        print("TEST 1: Detailed analysis (first 3 photos)")
        print("="*60)
        all_results['tests']['detailed_single'] = []
        
        for img in images[:3]:
            print(f"\n  === {img.name} ===")
            result = {
                'image': str(img.relative_to(SCRIPT_DIR)),
                'metadata': self.extract_exif(img),
                'analyses': {
                    'generic_tags': self.call_ollama(img, 'tags', use_thinking=False),
                    'detailed_tags': self.call_ollama(img, 'detailed_tags', use_thinking=False),
                    'brief_description': self.call_ollama(img, 'brief', use_thinking=False),
                    'full_description': self.call_ollama(img, 'description', use_thinking=True)
                }
            }
            all_results['tests']['detailed_single'].append(result)
        
        # TEST 2: Quick analysis of all photos
        print("\n" + "="*60)
        print("TEST 2: Quick analysis of all photos")
        print("="*60)
        all_results['tests']['quick_all'] = []
        
        for img in images:
            print(f"\n  Analyzing {img.name}...")
            result = {
                'image': str(img.relative_to(SCRIPT_DIR)),
                'metadata': self.extract_exif(img),
                'tags': self.call_ollama(img, 'tags', use_thinking=False)
            }
            all_results['tests']['quick_all'].append(result)
        
        # TEST 3: Group analysis without context hints
        print("\n" + "="*60)
        print("TEST 3: Group analysis (without hints)")
        print("="*60)
        group_result_no_context = self.analyze_photo_group(images[:7])
        all_results['tests']['group_no_context'] = group_result_no_context
        
        # TEST 4: Group analysis with context hints
        print("\n" + "="*60)
        print("TEST 4: Group analysis (with hints)")
        print("="*60)
        
        print("\nEnter context hints for this photo group")
        print("(e.g., 'Vacation in Barcelona, summer 2024, Gaudí architecture')")
        print("Or press ENTER to skip: ", end='')
        context = input().strip()
        
        if context:
            group_result_with_context = self.analyze_photo_group(images[7:], context)
            all_results['tests']['group_with_context'] = group_result_with_context
        
        # Save results
        model_safe_name = self.model_name.replace(':', '_').replace('/', '_')
        output_file = self.results_dir / f"test_results_{model_safe_name}_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
        with open(output_file, 'w', encoding='utf-8') as f:
            json.dump(all_results, f, indent=2, ensure_ascii=False)
        
        print("\n" + "="*60)
        print(f"Tests completed! Results saved in:")
        print(f"  {output_file.relative_to(SCRIPT_DIR)}")
        print("="*60)
        
        # Show summary
        self.print_summary(all_results)
        
        return all_results
    
    def print_summary(self, results: Dict):
        """Prints a summary of the results"""
        print("\n" + "="*60)
        print("RESULTS SUMMARY")
        print("="*60)
        
        # Test 1: Detailed samples
        if 'detailed_single' in results['tests']:
            print("\nTEST 1 - Detailed analysis (sample):")
            for r in results['tests']['detailed_single']:
                img_name = Path(r['image']).name
                print(f"\n  {img_name}:")
                if 'generic_tags' in r['analyses']:
                    tags = r['analyses']['generic_tags'].get('response', 'N/A')
                    print(f"    Generic tags: {tags[:100]}...")
        
        # Test 2: All photos
        if 'quick_all' in results['tests']:
            print(f"\nTEST 2 - Quick analysis:")
            print(f"  Processed {len(results['tests']['quick_all'])} photos")
        
        # Test 3: Group without context
        if 'group_no_context' in results['tests']:
            print(f"\nTEST 3 - Group without context:")
            group_tags = results['tests']['group_no_context']['group_analysis'].get('response', 'N/A')
            print(f"  Common tags: {group_tags[:100]}...")
        
        # Test 4: Group with context
        if 'group_with_context' in results['tests']:
            print(f"\nTEST 4 - Group with context:")
            group_tags = results['tests']['group_with_context']['group_analysis'].get('response', 'N/A')
            print(f"  Common tags: {group_tags[:100]}...")

def main():
    """Main entry point"""
    # Parse arguments
    parser = argparse.ArgumentParser(
        description='Test AI tagging system for photos',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Usage examples:
  %(prog)s                           # Use default model (qwen3-vl:8b)
  %(prog)s --model llava              # Use LLaVA
  %(prog)s --compare                  # Compare all models
  %(prog)s --list                     # List available models
        """
    )
    
    parser.add_argument(
        '--model', '-m',
        choices=list(MODEL_CONFIGS.keys()),
        help='Model to use'
    )
    
    parser.add_argument(
        '--compare', '-c',
        action='store_true',
        help='Compare all available models'
    )
    
    parser.add_argument(
        '--list', '-l',
        action='store_true',
        help='List available models and exit'
    )
    
    args = parser.parse_args()
    
    # List models
    if args.list:
        print("\n" + "="*60)
        print("AVAILABLE MODELS")
        print("="*60)
        for key, config in MODEL_CONFIGS.items():
            print(f"\n{key}:")
            print(f"  Name: {config['name']}")
            print(f"  Description: {config['description']}")
            print(f"  Temperature: {config['temperature']}")
        print("\n")
        return
    
    print("="*60)
    print("AI PHOTO TAGGING SYSTEM TEST")
    print("="*60)
    
    # Check that Ollama is available
    try:
        response = requests.get('http://localhost:11434/api/tags', timeout=5)
        if response.status_code != 200:
            print(f"\nERROR: Ollama not responding correctly (status {response.status_code})")
            print("Make sure Ollama is running: ollama serve")
            return
        
        # Get list of available models
        tags_data = response.json()
        available_models = []
        if 'models' in tags_data:
            available_models = [m.get('name', '') for m in tags_data['models']]
            
    except requests.RequestException as e:
        print(f"\nERROR: Cannot connect to Ollama: {e}")
        print("Make sure Ollama is running: ollama serve")
        return
    
    # Check that photo directory exists
    if not PHOTO_DIR.exists():
        print(f"\nERROR: Directory {PHOTO_DIR} not found!")
        print(f"Create the directory and insert test photos:")
        print(f"  mkdir -p {PHOTO_DIR}")
        return
    
    # Determine which model(s) to use
    models_to_test = []
    
    if args.compare:
        # Test all available models
        print("\nCOMPARE mode: will test all available models\n")
        models_to_test = list(MODEL_CONFIGS.keys())
    elif args.model:
        # Use specified model
        models_to_test = [args.model]
    else:
        # Use default
        models_to_test = [DEFAULT_MODEL]
    
    # Check that models are installed
    for model_key in models_to_test:
        model_name = MODEL_CONFIGS[model_key]['name']
        # Check if model is available
        model_found = any(model_name in am for am in available_models)
        if not model_found:
            print(f"\nWARNING: Model {model_name} not found!")
            print(f"Run: ollama pull {model_name}")
            print(f"Skipping this model...\n")
            models_to_test.remove(model_key)
    
    if not models_to_test:
        print("\nERROR: No models available for testing!")
        return
    
    # Run tests for each model
    for model_key in models_to_test:
        if len(models_to_test) > 1:
            print("\n" + "="*60)
            print(f"TESTING: {model_key}")
            print("="*60 + "\n")
        
        # Create analyzer and run tests
        analyzer = PhotoAnalyzer(PHOTO_DIR, RESULTS_DIR, model_key)
        analyzer.run_comprehensive_test()
        
        if len(models_to_test) > 1:
            print(f"\n{'='*60}")
            print(f"Completed test with {model_key}")
            print(f"{'='*60}\n")

if __name__ == "__main__":
    main()
