-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

--- Unit tests for JSON.lua encoder/decoder.
--- Run with: lua plugin/tests/test_json.lua

-- Resolve module paths relative to this script's location so that tests
-- work regardless of the current working directory.
local script_dir = arg[0]:match('(.*[/\\])') or './'
package.path = script_dir .. '../photometoria.lrdevplugin/?.lua;'
            .. script_dir .. '?.lua;'
            .. package.path

local JSON = require 'JSON'
local T = require 'testkit'

local test           = T.test
local assertEqual    = T.assertEqual
local assertNil      = T.assertNil
local assertNotNil   = T.assertNotNil
local assertTableLength = T.assertTableLength

T.suite('JSON decoder tests')

-- =============================================================================
-- Primitive types
-- =============================================================================

test('decode true', function()
	assertEqual(JSON.decode('true'), true)
end)

test('decode false', function()
	assertEqual(JSON.decode('false'), false)
end)

test('decode null', function()
	assertNil(JSON.decode('null'))
end)

-- =============================================================================
-- Numbers
-- =============================================================================

test('decode integer', function()
	assertEqual(JSON.decode('42'), 42)
end)

test('decode zero', function()
	assertEqual(JSON.decode('0'), 0)
end)

test('decode negative integer', function()
	assertEqual(JSON.decode('-7'), -7)
end)

test('decode float', function()
	assertEqual(JSON.decode('3.14'), 3.14)
end)

test('decode negative float', function()
	assertEqual(JSON.decode('-0.5'), -0.5)
end)

test('decode scientific notation', function()
	assertEqual(JSON.decode('1e10'), 1e10)
end)

test('decode negative exponent', function()
	assertEqual(JSON.decode('2.5e-3'), 2.5e-3)
end)

-- =============================================================================
-- Strings
-- =============================================================================

test('decode simple string', function()
	assertEqual(JSON.decode('"hello"'), 'hello')
end)

test('decode empty string', function()
	assertEqual(JSON.decode('""'), '')
end)

test('decode string with escaped quote', function()
	assertEqual(JSON.decode('"say \\"hi\\""'), 'say "hi"')
end)

test('decode string with backslash', function()
	assertEqual(JSON.decode('"a\\\\b"'), 'a\\b')
end)

test('decode string with newline escape', function()
	assertEqual(JSON.decode('"line1\\nline2"'), 'line1\nline2')
end)

test('decode string with tab escape', function()
	assertEqual(JSON.decode('"col1\\tcol2"'), 'col1\tcol2')
end)

test('decode string with carriage return', function()
	assertEqual(JSON.decode('"a\\rb"'), 'a\rb')
end)

test('decode string with forward slash escape', function()
	assertEqual(JSON.decode('"a\\/b"'), 'a/b')
end)

test('decode string with unicode escape (placeholder)', function()
	assertEqual(JSON.decode('"caf\\u00e9"'), 'caf?')
end)

-- =============================================================================
-- Arrays
-- =============================================================================

test('decode empty array', function()
	local arr = JSON.decode('[]')
	assertNotNil(arr, 'empty array')
	assertTableLength(arr, 0, 'empty array length')
end)

test('decode array of numbers', function()
	local arr = JSON.decode('[1, 2, 3]')
	assertEqual(arr[1], 1)
	assertEqual(arr[2], 2)
	assertEqual(arr[3], 3)
end)

test('decode array of strings', function()
	local arr = JSON.decode('["a", "b"]')
	assertEqual(arr[1], 'a')
	assertEqual(arr[2], 'b')
end)

test('decode array of mixed types', function()
	local arr = JSON.decode('[1, "two", true, false]')
	assertEqual(arr[1], 1)
	assertEqual(arr[2], 'two')
	assertEqual(arr[3], true)
	assertEqual(arr[4], false)
end)

-- Known limitation: JSON null inside arrays is lost because Lua nil
-- cannot be stored as an array element. A future improvement could
-- use a sentinel value (e.g. JSON.null) to represent null explicitly.
test('decode array with null (known limitation)', function()
	local arr = JSON.decode('[1, null, 3]')
	assertEqual(arr[1], 1)
	assertEqual(arr[2], 3, 'null is lost, element shifts down')
end)

test('decode nested arrays', function()
	local arr = JSON.decode('[[1, 2], [3, 4]]')
	assertEqual(arr[1][1], 1)
	assertEqual(arr[1][2], 2)
	assertEqual(arr[2][1], 3)
end)

-- =============================================================================
-- Objects
-- =============================================================================

test('decode empty object', function()
	local obj = JSON.decode('{}')
	assertNotNil(obj, 'empty object')
	assertTableLength(obj, 0, 'empty object length')
end)

test('decode simple object', function()
	local obj = JSON.decode('{"name": "test", "value": 42}')
	assertEqual(obj.name, 'test')
	assertEqual(obj.value, 42)
end)

test('decode nested object', function()
	local obj = JSON.decode('{"outer": {"inner": true}}')
	assertEqual(obj.outer.inner, true)
end)

test('decode object with array value', function()
	local obj = JSON.decode('{"items": [1, 2, 3]}')
	assertEqual(obj.items[1], 1)
	assertEqual(obj.items[2], 2)
	assertEqual(obj.items[3], 3)
end)

-- =============================================================================
-- Whitespace handling
-- =============================================================================

test('decode with leading whitespace', function()
	assertEqual(JSON.decode('  42'), 42)
end)

test('decode with trailing whitespace', function()
	assertEqual(JSON.decode('42  '), 42)
end)

test('decode pretty-printed JSON', function()
	local json = '{\n  "a": 1,\n  "b": 2\n}'
	local obj = JSON.decode(json)
	assertEqual(obj.a, 1)
	assertEqual(obj.b, 2)
end)

-- =============================================================================
-- Real-world: /api/info response
-- =============================================================================

test('decode /api/info response', function()
	local json = [[{
		"general": {
			"version": "0.1.0",
			"uptime_seconds": 123
		},
		"server": {
			"allocated_space_bytes": 1073741824,
			"used_space_bytes": 524288,
			"active_tasks_count": 2,
			"running_jobs_count": 1,
			"default_provider": "ollama",
			"available_providers": ["ollama"]
		}
	}]]

	local info = JSON.decode(json)
	assertNotNil(info, 'info should not be nil')
	assertEqual(info.general.version, '0.1.0')
	assertEqual(info.general.uptime_seconds, 123)
	assertEqual(info.server.allocated_space_bytes, 1073741824)
	assertEqual(info.server.used_space_bytes, 524288)
	assertEqual(info.server.active_tasks_count, 2)
	assertEqual(info.server.running_jobs_count, 1)
	assertEqual(info.server.default_provider, 'ollama')
	assertEqual(info.server.available_providers[1], 'ollama')
end)

-- =============================================================================
-- Encoder: primitive types
-- =============================================================================

T.suite('JSON encoder tests')

test('encode string', function()
	assertEqual(JSON.encode('hello'), '"hello"')
end)

test('encode empty string', function()
	assertEqual(JSON.encode(''), '""')
end)

test('encode string with quotes', function()
	assertEqual(JSON.encode('say "hi"'), '"say \\"hi\\""')
end)

test('encode string with backslash', function()
	assertEqual(JSON.encode('a\\b'), '"a\\\\b"')
end)

test('encode string with newline', function()
	assertEqual(JSON.encode('line1\nline2'), '"line1\\nline2"')
end)

test('encode string with tab', function()
	assertEqual(JSON.encode('col1\tcol2'), '"col1\\tcol2"')
end)

test('encode string with carriage return', function()
	assertEqual(JSON.encode('a\rb'), '"a\\rb"')
end)

test('encode integer', function()
	assertEqual(JSON.encode(42), '42')
end)

test('encode zero', function()
	assertEqual(JSON.encode(0), '0')
end)

test('encode negative number', function()
	assertEqual(JSON.encode(-7), '-7')
end)

test('encode float', function()
	assertEqual(JSON.encode(3.14), '3.14')
end)

test('encode true', function()
	assertEqual(JSON.encode(true), 'true')
end)

test('encode false', function()
	assertEqual(JSON.encode(false), 'false')
end)

test('encode nil', function()
	assertEqual(JSON.encode(nil), 'null')
end)

-- =============================================================================
-- Encoder: arrays
-- =============================================================================

test('encode array of numbers', function()
	assertEqual(JSON.encode({1, 2, 3}), '[1,2,3]')
end)

test('encode array of strings', function()
	assertEqual(JSON.encode({'a', 'b'}), '["a","b"]')
end)

test('encode array of mixed types', function()
	assertEqual(JSON.encode({1, 'two', true}), '[1,"two",true]')
end)

-- =============================================================================
-- Encoder: objects
-- =============================================================================

test('encode empty table', function()
	assertEqual(JSON.encode({}), '{}')
end)

test('encode simple object', function()
	local result = JSON.encode({name = 'test'})
	assertEqual(result, '{"name":"test"}')
end)

test('encode object with multiple keys', function()
	local result = JSON.encode({name = 'task', context = 'photos'})
	assertNotNil(result)
	local decoded = JSON.decode(result)
	assertEqual(decoded.name, 'task')
	assertEqual(decoded.context, 'photos')
end)

test('encode nested object', function()
	local result = JSON.encode({outer = {inner = true}})
	local decoded = JSON.decode(result)
	assertEqual(decoded.outer.inner, true)
end)

test('encode object with array value', function()
	local result = JSON.encode({items = {1, 2, 3}})
	local decoded = JSON.decode(result)
	assertEqual(decoded.items[1], 1)
	assertEqual(decoded.items[2], 2)
	assertEqual(decoded.items[3], 3)
end)

-- =============================================================================
-- Encoder: round-trip (encode then decode)
-- =============================================================================

test('round-trip: task creation payload', function()
	local original = {name = 'Summer vacation', context = 'Photos from San Francisco'}
	local json = JSON.encode(original)
	local decoded = JSON.decode(json)
	assertEqual(decoded.name, original.name)
	assertEqual(decoded.context, original.context)
end)

test('round-trip: string with special characters', function()
	local original = 'line1\nline2\ttab "quoted" back\\slash'
	local json = JSON.encode(original)
	local decoded = JSON.decode(json)
	assertEqual(decoded, original)
end)

-- =============================================================================
-- Encoder: error handling
-- =============================================================================

test('encode returns string for valid input', function()
	local result = JSON.encode({a = 1})
	assertEqual(type(result), 'string', 'encode should return string')
end)

-- =============================================================================
-- Decoder: error handling
-- =============================================================================

test('decode nil input returns error', function()
	local val, err = JSON.decode(nil)
	assertNil(val, 'nil input value')
	assertEqual(err, 'invalid input')
end)

test('decode empty string returns error', function()
	local val, err = JSON.decode('')
	assertNil(val, 'empty string value')
	assertEqual(err, 'invalid input')
end)

test('decode non-string returns error', function()
	local val, err = JSON.decode(123)
	assertNil(val, 'numeric input value')
	assertEqual(err, 'invalid input')
end)

-- =============================================================================

T.report()
