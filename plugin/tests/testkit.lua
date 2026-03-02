-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

--- Minimal test framework for Lua 5.1 unit tests.
--- Designed to run standalone, without Lightroom SDK dependencies.

local testkit = {}

local passed = 0
local failed = 0
local errors = {}
local suiteName = 'tests'

--- Sets a label shown in the final summary line.
function testkit.suite(name)
	suiteName = name
end

--- Registers and runs a single test case.
function testkit.test(name, fn)
	local ok, err = pcall(fn)
	if ok then
		passed = passed + 1
	else
		failed = failed + 1
		errors[#errors + 1] = string.format('  FAIL: %s\n        %s', name, tostring(err))
	end
end

--- Asserts that actual == expected.
function testkit.assertEqual(actual, expected, msg)
	if actual ~= expected then
		error(string.format('%s: expected %s, got %s',
			msg or 'assertion failed',
			tostring(expected),
			tostring(actual)))
	end
end

--- Asserts that actual is nil.
function testkit.assertNil(actual, msg)
	if actual ~= nil then
		error(string.format('%s: expected nil, got %s',
			msg or 'assertion failed',
			tostring(actual)))
	end
end

--- Asserts that actual is not nil.
function testkit.assertNotNil(actual, msg)
	if actual == nil then
		error(string.format('%s: expected non-nil value', msg or 'assertion failed'))
	end
end

--- Asserts that table t has exactly expected entries (via pairs).
function testkit.assertTableLength(t, expected, msg)
	local count = 0
	for _ in pairs(t) do count = count + 1 end
	if count ~= expected then
		error(string.format('%s: expected %d entries, got %d',
			msg or 'table length',
			expected, count))
	end
end

--- Prints the test summary and exits with code 1 on failure.
function testkit.report()
	print(string.format('\n%s: %d passed, %d failed, %d total',
		suiteName, passed, failed, passed + failed))

	if #errors > 0 then
		print('')
		for _, e in ipairs(errors) do
			print(e)
		end
		print('')
		os.exit(1)
	end
end

return testkit
