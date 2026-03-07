-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrHttp = import 'LrHttp'
local LrTasks = import 'LrTasks'

local JSON = require 'JSON'

local ServerConnection = {}

local TIMEOUT_SECONDS = 10

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

--- Formats a byte count into a human-readable string.
local function formatBytes(bytes)
	if bytes < 1024 then
		return string.format('%d B', bytes)
	elseif bytes < 1024 * 1024 then
		return string.format('%.1f KB', bytes / 1024)
	elseif bytes < 1024 * 1024 * 1024 then
		return string.format('%.1f MB', bytes / (1024 * 1024))
	else
		return string.format('%.1f GB', bytes / (1024 * 1024 * 1024))
	end
end

--- Fetches server info synchronously. Must be called from within an async task.
--- Returns `(true, data)` on success or `(false, data)` on failure.
--- On success, data contains server details formatted for display.
--- On failure, data contains a `message` field with the error description.
function ServerConnection.fetch(host)
	local url = 'http://' .. host .. '/api/info'

	local body, headers = LrHttp.get(url, nil, TIMEOUT_SECONDS)

	if not body or not headers then
		return false, {
			message = LOC("$$$/Photometoria/Status/CannotReach=Could not reach server at ^1", host),
		}
	end

	if headers.status ~= 200 then
		return false, {
			message = LOC("$$$/Photometoria/Status/ServerError=Server responded with status ^1", tostring(headers.status)),
		}
	end

	local info, err = JSON.decode(body)
	if not info or not info.server then
		return false, {
			message = LOC "$$$/Photometoria/Status/InvalidResponse=Invalid response from server",
		}
	end

	local server = info.server
	local allocated = server.allocated_space_bytes or 0
	local used = server.used_space_bytes or 0
	local free = (allocated > used) and (allocated - used) or 0

	local providers = table.concat(server.available_providers or {}, ', ')
	local defaultProvider = server.default_provider or ''

	return true, {
		storageAllocated = formatBytes(allocated),
		storageUsed      = formatBytes(used),
		storageFree      = formatBytes(free),
		providers        = providers,
		defaultProvider  = defaultProvider,
		version          = info.general and info.general.version or '',
		activeTasks      = server.active_tasks_count or 0,
		queuedJobs       = server.running_jobs_count or 0,
	}
end

--- Connects to a Photometoria server and retrieves its info.
--- Calls `callback(success, data)` asynchronously.
function ServerConnection.connect(host, callback)
	LrTasks.startAsyncTask(function()
		local success, data = ServerConnection.fetch(host)
		callback(success, data)
	end)
end

return ServerConnection
