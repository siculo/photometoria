-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

--- Minimal JSON encoder/decoder for Lightroom plugin use.
--- Supports objects, arrays, strings, numbers, booleans, and null.

local JSON = {}

local function skipWhitespace(str, pos)
	return str:match('^%s*()', pos)
end

local function decodeString(str, pos)
	if str:sub(pos, pos) ~= '"' then
		return nil, pos
	end

	local result = {}
	local i = pos + 1
	while i <= #str do
		local c = str:sub(i, i)
		if c == '"' then
			return table.concat(result), i + 1
		elseif c == '\\' then
			i = i + 1
			local esc = str:sub(i, i)
			if esc == '"' or esc == '\\' or esc == '/' then
				result[#result + 1] = esc
			elseif esc == 'n' then
				result[#result + 1] = '\n'
			elseif esc == 'r' then
				result[#result + 1] = '\r'
			elseif esc == 't' then
				result[#result + 1] = '\t'
			elseif esc == 'u' then
				result[#result + 1] = '?'
				i = i + 4
			end
		else
			result[#result + 1] = c
		end
		i = i + 1
	end

	return nil, pos
end

local decodeValue

local function decodeNumber(str, pos)
	local numStr = str:match('^%-?%d+%.?%d*[eE]?[%+%-]?%d*()', pos)
	if not numStr then
		return nil, pos
	end

	local num = tonumber(str:sub(pos, numStr - 1))
	return num, numStr
end

local function decodeArray(str, pos)
	if str:sub(pos, pos) ~= '[' then
		return nil, pos
	end

	local arr = {}
	pos = skipWhitespace(str, pos + 1)

	if str:sub(pos, pos) == ']' then
		return arr, pos + 1
	end

	while true do
		local val
		val, pos = decodeValue(str, pos)
		arr[#arr + 1] = val

		pos = skipWhitespace(str, pos)
		local c = str:sub(pos, pos)
		if c == ']' then
			return arr, pos + 1
		elseif c == ',' then
			pos = skipWhitespace(str, pos + 1)
		else
			return nil, pos
		end
	end
end

local function decodeObject(str, pos)
	if str:sub(pos, pos) ~= '{' then
		return nil, pos
	end

	local obj = {}
	pos = skipWhitespace(str, pos + 1)

	if str:sub(pos, pos) == '}' then
		return obj, pos + 1
	end

	while true do
		local key
		key, pos = decodeString(str, pos)
		if not key then
			return nil, pos
		end

		pos = skipWhitespace(str, pos)
		if str:sub(pos, pos) ~= ':' then
			return nil, pos
		end
		pos = skipWhitespace(str, pos + 1)

		local val
		val, pos = decodeValue(str, pos)
		obj[key] = val

		pos = skipWhitespace(str, pos)
		local c = str:sub(pos, pos)
		if c == '}' then
			return obj, pos + 1
		elseif c == ',' then
			pos = skipWhitespace(str, pos + 1)
		else
			return nil, pos
		end
	end
end

function decodeValue(str, pos)
	pos = skipWhitespace(str, pos)
	local c = str:sub(pos, pos)

	if c == '"' then
		return decodeString(str, pos)
	elseif c == '{' then
		return decodeObject(str, pos)
	elseif c == '[' then
		return decodeArray(str, pos)
	elseif c == 't' then
		if str:sub(pos, pos + 3) == 'true' then
			return true, pos + 4
		end
	elseif c == 'f' then
		if str:sub(pos, pos + 4) == 'false' then
			return false, pos + 5
		end
	elseif c == 'n' then
		if str:sub(pos, pos + 3) == 'null' then
			return nil, pos + 4
		end
	else
		return decodeNumber(str, pos)
	end

	return nil, pos
end

--- Decodes a JSON string into a Lua table.
--- Returns the decoded value, or nil and an error message on failure.
function JSON.decode(str)
	if type(str) ~= 'string' or str == '' then
		return nil, 'invalid input'
	end

	local ok, result = pcall(function()
		local val, pos = decodeValue(str, 1)
		return val
	end)

	if ok then
		return result
	else
		return nil, 'JSON parse error'
	end
end

local function encodeString(s)
	s = s:gsub('\\', '\\\\')
	s = s:gsub('"', '\\"')
	s = s:gsub('\n', '\\n')
	s = s:gsub('\r', '\\r')
	s = s:gsub('\t', '\\t')
	return '"' .. s .. '"'
end

local encodeValue

local function encodeTable(tbl)
	if #tbl > 0 then
		local parts = {}
		for i = 1, #tbl do
			parts[i] = encodeValue(tbl[i])
		end
		return '[' .. table.concat(parts, ',') .. ']'
	end

	local parts = {}
	for k, v in pairs(tbl) do
		if type(k) == 'string' then
			parts[#parts + 1] = encodeString(k) .. ':' .. encodeValue(v)
		end
	end
	return '{' .. table.concat(parts, ',') .. '}'
end

encodeValue = function(val)
	if val == nil then
		return 'null'
	end

	local t = type(val)
	if t == 'boolean' then
		return val and 'true' or 'false'
	elseif t == 'number' then
		return tostring(val)
	elseif t == 'string' then
		return encodeString(val)
	elseif t == 'table' then
		return encodeTable(val)
	end
	return 'null'
end

--- Encodes a Lua value into a JSON string.
--- Returns the JSON string, or nil and an error message on failure.
function JSON.encode(value)
	local ok, result = pcall(function()
		return encodeValue(value)
	end)

	if ok then
		return result
	else
		return nil, 'JSON encode error'
	end
end

return JSON
