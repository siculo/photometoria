-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

package.path = 'plugin/photometoria.lrdevplugin/?.lua;plugin/tests/?.lua;' .. package.path

--- Mock for Lightroom LOC function.
--- Extracts the default value after '=' and substitutes ^1, ^2, etc.
function LOC(str, ...)
	local default = str:match('=(.+)$')
	if not default then return str end
	local args = {...}
	for i, arg in ipairs(args) do
		default = default:gsub('%^' .. i, tostring(arg))
	end
	return default
end

local testkit = require 'testkit'
local PhotoValidator = require 'PhotoValidator'

testkit.suite('PhotoValidator')

testkit.test('formatSummary: all valid', function()
	local result = { total = 47, uploadable = 47, missing = 0, videos = 0 }
	testkit.assertEqual(PhotoValidator.formatSummary(result), '47 photos')
end)

testkit.test('formatSummary: some missing', function()
	local result = { total = 47, uploadable = 45, missing = 2, videos = 0 }
	testkit.assertEqual(
		PhotoValidator.formatSummary(result),
		'47 photos \194\183 45 uploadable (2 missing)')
end)

testkit.test('formatSummary: some videos', function()
	local result = { total = 47, uploadable = 46, missing = 0, videos = 1 }
	testkit.assertEqual(
		PhotoValidator.formatSummary(result),
		'47 photos \194\183 46 uploadable (1 video)')
end)

testkit.test('formatSummary: missing and videos', function()
	local result = { total = 47, uploadable = 44, missing = 2, videos = 1 }
	testkit.assertEqual(
		PhotoValidator.formatSummary(result),
		'47 photos \194\183 44 uploadable (2 missing, 1 video)')
end)

testkit.test('formatSummary: no target photos', function()
	local result = { total = 0, uploadable = 0, missing = 0, videos = 0 }
	testkit.assertEqual(PhotoValidator.formatSummary(result), 'No target photos')
end)

testkit.test('formatSummary: none uploadable', function()
	local result = { total = 3, uploadable = 0, missing = 2, videos = 1 }
	testkit.assertEqual(
		PhotoValidator.formatSummary(result),
		'3 photos \194\183 0 uploadable (2 missing, 1 video)')
end)

testkit.test('formatSummary: all missing', function()
	local result = { total = 5, uploadable = 0, missing = 5, videos = 0 }
	testkit.assertEqual(
		PhotoValidator.formatSummary(result),
		'5 photos \194\183 0 uploadable (5 missing)')
end)

testkit.test('formatSummary: all videos', function()
	local result = { total = 4, uploadable = 0, missing = 0, videos = 4 }
	testkit.assertEqual(
		PhotoValidator.formatSummary(result),
		'4 photos \194\183 0 uploadable (4 video)')
end)

testkit.report()
