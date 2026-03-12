-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrHttp = import 'LrHttp'
local LrTasks = import 'LrTasks'

local JSON = require 'JSON'

local ServerConnection = {}

local TIMEOUT_SECONDS = 10
local UPLOAD_TIMEOUT_SECONDS = 120

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

--- Retrieves server info synchronously. Must be called from within an async task.
--- Returns `(true, data)` on success or `(false, data)` on failure.
--- On success, data contains server details formatted for display.
--- On failure, data contains a `message` field with the error description.
function ServerConnection.info(host)
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

	local limits = info.limits or {}

	return true, {
		storageAllocated    = formatBytes(allocated),
		storageUsed         = formatBytes(used),
		storageFree         = formatBytes(free),
		storageFreeBytes    = free,
		providers           = providers,
		defaultProvider     = defaultProvider,
		version             = info.general and info.general.version or '',
		activeTasks         = server.active_tasks_count or 0,
		queuedJobs          = server.running_jobs_count or 0,
		maxPhotosPerRequest = limits.max_photos_per_request or 100,
		maxPhotoSizeBytes   = limits.max_photo_size_bytes or 0,
	}
end

--- Creates a new task on the server. Must be called from within an async task.
--- Returns `(true, data)` on success or `(false, data)` on failure.
--- On success, data contains the task details (task_id, name, context, created_at).
--- On failure, data contains `message` and optionally `duplicate = true` for 409.
function ServerConnection.createTask(host, name, context)
	local url = 'http://' .. host .. '/api/tasks'
	local body = JSON.encode({ name = name, context = context })

	local headers = {
		{ field = 'Content-Type', value = 'application/json' },
	}

	local respBody, respHeaders = LrHttp.post(url, body, headers, 'POST', TIMEOUT_SECONDS)

	if not respBody or not respHeaders then
		return false, {
			message = LOC("$$$/Photometoria/Status/CannotReach=Could not reach server at ^1", host),
		}
	end

	if respHeaders.status == 201 then
		local data = JSON.decode(respBody)
		return true, data or {}
	end

	if respHeaders.status == 409 then
		return false, {
			message = LOC "$$$/Photometoria/Error/DuplicateName=A task with this name already exists. Please choose a different name.",
			duplicate = true,
		}
	end

	return false, {
		message = LOC("$$$/Photometoria/Status/ServerError=Server responded with status ^1", tostring(respHeaders.status)),
	}
end

--- Performs a GET request and decodes the JSON response.
--- Returns `(true, data)` on success or `(false, error)` on failure.
local function getJson(host, path)
	local url = 'http://' .. host .. path

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

	local data = JSON.decode(body)
	if not data then
		return false, {
			message = LOC "$$$/Photometoria/Status/InvalidResponse=Invalid response from server",
		}
	end

	return true, data
end

--- Retrieves the list of tasks from the server. Must be called from within an async task.
--- Returns `(true, tasks)` on success or `(false, error)` on failure.
--- On success, tasks is an array of TaskSummary objects with fields:
--- task_id, name, context, photo_count, storage_used, created_at, job_count.
function ServerConnection.listTasks(host)
	return getJson(host, '/api/tasks')
end

--- Retrieves the list of jobs for a task. Must be called from within an async task.
--- Returns `(true, jobs)` on success or `(false, error)` on failure.
--- On success, jobs is an array of JobSummary objects with fields:
--- job_id, status, model, photo_count, queued_photo_count, processed_photo_count,
--- created_at, completed_at.
function ServerConnection.listTaskJobs(host, taskId)
	return getJson(host, '/api/tasks/' .. taskId .. '/jobs')
end

--- Retrieves the list of AI providers. Must be called from within an async task.
--- Returns `(true, data)` on success or `(false, error)` on failure.
--- On success, data contains `providers` (array of {name}) and `default` (string or nil).
function ServerConnection.listProviders(host)
	return getJson(host, '/api/providers')
end

--- Retrieves details for a single provider, including models and availability.
--- Must be called from within an async task.
--- Returns `(true, data)` on success or `(false, error)` on failure.
--- On success, data contains `name` and `models` (array of {name, description, available}).
function ServerConnection.providerDetails(host, providerName)
	return getJson(host, '/api/providers/' .. providerName)
end

--- Creates a new job for a task. Must be called from within an async task.
--- Returns `(true, data)` on success or `(false, data)` on failure.
--- On success, data contains job_id, task_id, status, model, photo_count, created_at.
--- On failure, data contains `message` with the error description.
function ServerConnection.createJob(host, taskId, model)
	local url = 'http://' .. host .. '/api/tasks/' .. taskId .. '/jobs'
	local body = JSON.encode({ model = model })

	local headers = {
		{ field = 'Content-Type', value = 'application/json' },
	}

	local respBody, respHeaders = LrHttp.post(url, body, headers, 'POST', TIMEOUT_SECONDS)

	if not respBody or not respHeaders then
		return false, {
			message = LOC("$$$/Photometoria/Status/CannotReach=Could not reach server at ^1", host),
		}
	end

	if respHeaders.status == 200 or respHeaders.status == 201 then
		local data = JSON.decode(respBody)
		return true, data or {}
	end

	if respHeaders.status == 404 then
		return false, {
			message = LOC "$$$/Photometoria/Error/TaskNotFound=Task not found on the server.",
		}
	end

	if respHeaders.status == 400 then
		local data = JSON.decode(respBody)
		if data and data.message then
			return false, { message = data.message }
		end
	end

	return false, {
		message = LOC("$$$/Photometoria/Status/ServerError=Server responded with status ^1", tostring(respHeaders.status)),
	}
end

--- Uploads photos to a task via multipart form data. Must be called from an async task.
--- files: array of { clientId, filePath, fileName }.
--- Returns `(true, data)` on success or `(false, error)` on failure.
--- On success, data contains `uploaded` and `failed` arrays as returned by the server.
function ServerConnection.uploadPhotos(host, taskId, files)
	local url = 'http://' .. host .. '/api/tasks/' .. taskId .. '/photos'

	local clientIds = {}
	for _, file in ipairs(files) do
		clientIds[#clientIds + 1] = file.clientId
	end

	local content = {
		{ name = 'client_ids', value = JSON.encode(clientIds) },
	}
	for _, file in ipairs(files) do
		content[#content + 1] = {
			name = 'files',
			filePath = file.filePath,
			fileName = file.fileName,
			contentType = 'image/jpeg',
		}
	end

	local respBody, respHeaders = LrHttp.postMultipart(url, content, nil, UPLOAD_TIMEOUT_SECONDS)

	if not respBody or not respHeaders then
		return false, {
			message = LOC("$$$/Photometoria/Status/CannotReach=Could not reach server at ^1", host),
		}
	end

	if respHeaders.status == 201 or respHeaders.status == 200 then
		local data = JSON.decode(respBody)
		return true, data or { uploaded = {}, failed = {} }
	end

	return false, {
		message = LOC("$$$/Photometoria/Status/ServerError=Server responded with status ^1", tostring(respHeaders.status)),
	}
end

--- Updates a task on the server. Must be called from within an async task.
--- Returns `(true, data)` on success or `(false, data)` on failure.
--- On success, data contains the updated task (task_id, name, context, created_at).
--- On failure, data contains `message` and optionally `duplicate = true` for 409.
function ServerConnection.updateTask(host, taskId, name, context)
	local url = 'http://' .. host .. '/api/tasks/' .. taskId
	local body = JSON.encode({ name = name, context = context })

	local headers = {
		{ field = 'Content-Type', value = 'application/json' },
	}

	local respBody, respHeaders = LrHttp.post(url, body, headers, 'PATCH', TIMEOUT_SECONDS)

	if not respBody or not respHeaders then
		return false, {
			message = LOC("$$$/Photometoria/Status/CannotReach=Could not reach server at ^1", host),
		}
	end

	if respHeaders.status == 200 then
		local data = JSON.decode(respBody)
		return true, data or {}
	end

	if respHeaders.status == 404 then
		return false, {
			message = LOC "$$$/Photometoria/Error/TaskNotFound=Task not found on the server.",
		}
	end

	if respHeaders.status == 409 then
		return false, {
			message = LOC "$$$/Photometoria/Error/DuplicateName=A task with this name already exists. Please choose a different name.",
			duplicate = true,
		}
	end

	return false, {
		message = LOC("$$$/Photometoria/Status/ServerError=Server responded with status ^1", tostring(respHeaders.status)),
	}
end

--- Retrieves server info asynchronously.
--- Calls `callback(success, data)` when done.
function ServerConnection.infoAsync(host, callback)
	LrTasks.startAsyncTask(function()
		local success, data = ServerConnection.info(host)
		callback(success, data)
	end)
end

return ServerConnection
