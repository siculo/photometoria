-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local MockData = {}

MockData.tasks = {
	{
		id = 'task-001',
		name = 'Matrimonio Villa Borghese',
		photoCount = 142,
		sizeBytes = 2684354560,
		status = 'active',
		context = 'Wedding reception at Villa Borghese, Rome. Outdoor ceremony in the garden followed by indoor dinner. Key subjects: bride and groom, family portraits, table settings, venue architecture. Style: classic editorial with warm tones.',
		jobs = {
			{
				id = 'job-001a',
				provider = 'Ollama',
				model = 'qwen2-vl:8b',
				status = 'running',
				photosTotal = 142,
				photosProcessed = 87,
				estimatedRemaining = '12 min',
				duration = '',
				errorCount = 0,
			},
			{
				id = 'job-001b',
				provider = 'Ollama',
				model = 'llava:13b',
				status = 'completed',
				photosTotal = 142,
				photosProcessed = 142,
				estimatedRemaining = '',
				duration = '48 min',
				errorCount = 0,
			},
		},
	},
	{
		id = 'task-002',
		name = 'Portfolio Architettura',
		photoCount = 56,
		sizeBytes = 1073741824,
		status = 'completed',
		context = 'Architectural photography portfolio. Modern buildings in Milan business district. Focus on geometry, reflections, leading lines. Shot during golden hour and blue hour.',
		jobs = {
			{
				id = 'job-002a',
				provider = 'Ollama',
				model = 'qwen2-vl:8b',
				status = 'completed',
				photosTotal = 56,
				photosProcessed = 56,
				estimatedRemaining = '',
				duration = '18 min',
				errorCount = 0,
			},
		},
	},
	{
		id = 'task-003',
		name = 'Street Photography Tokyo',
		photoCount = 230,
		sizeBytes = 4294967296,
		status = 'errors',
		context = 'Street photography in Shibuya and Shinjuku, Tokyo. Night scenes with neon lights, rainy reflections, crowds. Black and white conversions mixed with selective color.',
		jobs = {
			{
				id = 'job-003a',
				provider = 'Ollama',
				model = 'qwen2-vl:8b',
				status = 'errored',
				photosTotal = 230,
				photosProcessed = 215,
				estimatedRemaining = '',
				duration = '1h 12 min',
				errorCount = 15,
			},
			{
				id = 'job-003b',
				provider = 'Ollama',
				model = 'llava:13b',
				status = 'cancelled',
				photosTotal = 230,
				photosProcessed = 45,
				estimatedRemaining = '',
				duration = '14 min',
				errorCount = 0,
			},
		},
	},
}

MockData.providers = {
	{
		name = 'Ollama',
		models = { 'qwen2-vl:8b', 'llava:latest' },
		estimatedCost = nil,
	},
	{
		name = 'OpenAI',
		models = { 'gpt-4o', 'gpt-4-vision-preview' },
		estimatedCost = '\226\130\172 0.86',
	},
	{
		name = 'Anthropic',
		models = { 'claude-3-5-sonnet-20241022', 'claude-3-opus-20240229' },
		estimatedCost = '\226\130\172 1.20',
	},
}

MockData.selectedPhotos = {
	count = 47,
	sizeBytes = 1288490189,
}

MockData.allPhotos = {
	count = 312,
	sizeBytes = 9018753434,
}

return MockData
