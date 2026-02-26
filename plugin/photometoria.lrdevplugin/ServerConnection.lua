-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrTasks = import 'LrTasks'

local ServerConnection = {}

--- Validates that a string matches the host:port format.
--- Accepts IPv4 addresses and hostnames with a numeric port (1-65535).
function ServerConnection.isValidHostPort(value)
	if type(value) ~= 'string' or value == '' then
		return false
	end

	local host, portStr = value:match('^([^:]+):(%d+)$')
	if not host or not portStr then
		return false
	end

	local port = tonumber(portStr)
	if not port or port < 1 or port > 65535 then
		return false
	end

	if host:match('^%d+%.%d+%.%d+%.%d+$') then
		for octet in host:gmatch('%d+') do
			local n = tonumber(octet)
			if not n or n < 0 or n > 255 then
				return false
			end
		end
		return true
	end

	if host:match('^[%w][%w%-%.]*[%w]$') or host:match('^%w$') then
		return true
	end

	return false
end

--- Simulates a server connection attempt with a delay.
--- Calls `callback(success, data)` where data contains server details on success
--- or an error message on failure.
function ServerConnection.simulateConnect(host, callback)
	LrTasks.startAsyncTask(function()
		LrTasks.sleep(0.9)

		if host == '192.168.1.50:8080' then
			callback(true, {
				storageAllocated = '100 GB',
				storageUsed     = '23.5 GB',
				storageFree     = '62.4 GB',
				providers       = 'Ollama, LocalAI, OpenAI',
				defaultProvider = 'Ollama → qwen2-vl:8b',
				version         = 'v0.3.1',
				activeTasks     = 2,
				queuedJobs      = 1,
			})
		else
			callback(false, {
				message = 'Could not reach server at ' .. host,
			})
		end
	end)
end

return ServerConnection
