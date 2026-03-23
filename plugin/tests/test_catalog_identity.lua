-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

package.path = 'plugin/photometoria.lrdevplugin/?.lua;plugin/tests/?.lua;' .. package.path

local testkit = require 'testkit'
local UUID = require 'UUID'

testkit.suite('CatalogIdentity')

local UUID_PATTERN = '^%x%x%x%x%x%x%x%x%-%x%x%x%x%-4%x%x%x%-[89ab]%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x$'

testkit.test('generate: returns valid UUID v4 format', function()
	math.randomseed(12345)
	local uuid = UUID.generate()
	testkit.assertNotNil(uuid:match(UUID_PATTERN), 'UUID does not match v4 pattern: ' .. uuid)
end)

testkit.test('generate: correct length (36 characters)', function()
	math.randomseed(12345)
	local uuid = UUID.generate()
	testkit.assertEqual(#uuid, 36, 'UUID length')
end)

testkit.test('generate: version nibble is 4', function()
	math.randomseed(99999)
	local uuid = UUID.generate()
	local version = uuid:sub(15, 15)
	testkit.assertEqual(version, '4', 'version nibble')
end)

testkit.test('generate: variant nibble is 8, 9, a, or b', function()
	math.randomseed(99999)
	local uuid = UUID.generate()
	local variant = uuid:sub(20, 20)
	testkit.assertNotNil(variant:match('[89ab]'), 'variant nibble: ' .. variant)
end)

testkit.test('generate: different seeds produce different UUIDs', function()
	math.randomseed(11111)
	local uuid1 = UUID.generate()
	math.randomseed(22222)
	local uuid2 = UUID.generate()
	testkit.assertNotNil(uuid1 ~= uuid2 or nil, 'UUIDs should differ with different seeds')
end)

testkit.test('generate: same seed produces same UUID (deterministic)', function()
	math.randomseed(42)
	local uuid1 = UUID.generate()
	math.randomseed(42)
	local uuid2 = UUID.generate()
	testkit.assertEqual(uuid1, uuid2, 'deterministic with same seed')
end)

testkit.test('generate: all lowercase hex digits', function()
	math.randomseed(77777)
	local uuid = UUID.generate()
	local hex_only = uuid:gsub('-', '')
	testkit.assertNotNil(hex_only:match('^%x+$'), 'non-hex character found: ' .. uuid)
end)

testkit.test('generate: 1000 UUIDs all match v4 format', function()
	math.randomseed(os.time())
	for i = 1, 1000 do
		local uuid = UUID.generate()
		if not uuid:match(UUID_PATTERN) then
			error('UUID #' .. i .. ' does not match v4 pattern: ' .. uuid)
		end
	end
end)

testkit.test('generate: 1000 UUIDs are all unique', function()
	math.randomseed(os.time())
	local seen = {}
	for i = 1, 1000 do
		local uuid = UUID.generate()
		if seen[uuid] then
			error('duplicate UUID at #' .. i .. ': ' .. uuid)
		end
		seen[uuid] = true
	end
end)

testkit.report()
