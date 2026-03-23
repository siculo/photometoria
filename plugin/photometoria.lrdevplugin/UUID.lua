-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

--- Pure-Lua UUID v4 generator (RFC 4122).
--- No Lightroom SDK dependencies — testable standalone.

local UUID = {}

--- Generates a UUID v4 string using math.random.
--- The PRNG must be seeded before calling this function.
function UUID.generate()
	local template = 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'
	local uuid = template:gsub('[xy]', function(c)
		local v = (c == 'x') and math.random(0, 15) or math.random(8, 11)
		return string.format('%x', v)
	end)
	return uuid
end

return UUID
