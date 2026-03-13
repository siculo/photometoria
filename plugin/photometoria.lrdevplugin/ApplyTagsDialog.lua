-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrApplication = import 'LrApplication'
local LrDialogs = import 'LrDialogs'
local LrTasks = import 'LrTasks'

local ServerConnection = require 'ServerConnection'

local ApplyTagsDialog = {}

--- Splits a comma-separated tag string into an array of trimmed tag names.
local function parseTags(tagString)
	local tags = {}
	for tag in tagString:gmatch('[^,]+') do
		local trimmed = tag:match('^%s*(.-)%s*$')
		if trimmed ~= '' then
			tags[#tags + 1] = trimmed
		end
	end
	return tags
end

--- Resolves job results to catalog photos, separating matched from unmatched.
--- Returns (matched, failedCount, missingCount) where matched is an array of
--- {photo, tags} and missingCount counts photos no longer in the catalog.
local function resolveResults(results)
	local catalog = LrApplication.activeCatalog()
	local matched = {}
	local failedCount = 0
	local missingCount = 0

	for _, result in ipairs(results) do
		if result.status ~= 'completed' or not result.tags then
			failedCount = failedCount + 1
		elseif not result.client_id then
			missingCount = missingCount + 1
		else
			local photo = catalog:findPhotoByUuid(result.client_id)
			if photo then
				matched[#matched + 1] = {
					photo = photo,
					tags = parseTags(result.tags),
				}
			else
				missingCount = missingCount + 1
			end
		end
	end

	return matched, failedCount, missingCount
end

--- Applies keyword tags to matched photos inside a catalog write transaction.
--- Returns the number of photos that received keywords.
local function applyKeywords(matched)
	local catalog = LrApplication.activeCatalog()
	local appliedCount = 0

	catalog:withWriteAccessDo(
		LOC "$$$/Photometoria/Undo/ApplyTags=Photometoria: Apply tags",
		function()
			for _, entry in ipairs(matched) do
				for _, tagName in ipairs(entry.tags) do
					local keyword = catalog:createKeyword(tagName, {}, true, nil, true)
					if keyword then
						entry.photo:addKeyword(keyword)
					end
				end
				appliedCount = appliedCount + 1
			end
		end
	)

	return appliedCount
end

--- Builds a confirmation message describing what will be applied.
local function buildConfirmMessage(matched, failedCount, missingCount)
	local lines = {}

	lines[#lines + 1] = LOC(
		"$$$/Photometoria/ApplyTags/PhotoCount=Tags will be applied to ^1 photos.",
		tostring(#matched)
	)

	if failedCount > 0 then
		lines[#lines + 1] = LOC(
			"$$$/Photometoria/ApplyTags/FailedCount=^1 photos did not produce results.",
			tostring(failedCount)
		)
	end

	if missingCount > 0 then
		lines[#lines + 1] = LOC(
			"$$$/Photometoria/ApplyTags/MissingCount=^1 photos were not found in the catalog.",
			tostring(missingCount)
		)
	end

	return table.concat(lines, '\n')
end

--- Fetches job results, shows a confirmation dialog, and applies tags.
--- Must be called from within an async task context.
--- host: server address (host:port)
--- jobId: the job ID to fetch results for
function ApplyTagsDialog.run(host, jobId)
	local ok, data = ServerConnection.getJobResults(host, jobId)
	if not ok then
		LrDialogs.message(
			LOC "$$$/Photometoria/ApplyTags/ErrorTitle=Apply tags",
			data and data.message or LOC "$$$/Photometoria/ApplyTags/FetchError=Could not retrieve job results.",
			'critical'
		)
		return
	end

	local results = data.results or {}
	if #results == 0 then
		LrDialogs.message(
			LOC "$$$/Photometoria/ApplyTags/ErrorTitle=Apply tags",
			LOC "$$$/Photometoria/ApplyTags/NoResults=No results available for this job.",
			'info'
		)
		return
	end

	local matched, failedCount, missingCount = resolveResults(results)

	if #matched == 0 then
		LrDialogs.message(
			LOC "$$$/Photometoria/ApplyTags/ErrorTitle=Apply tags",
			LOC "$$$/Photometoria/ApplyTags/NoMatched=No photos with tags were found in the catalog.",
			'warning'
		)
		return
	end

	local message = buildConfirmMessage(matched, failedCount, missingCount)
	local confirm = LrDialogs.confirm(
		LOC "$$$/Photometoria/ApplyTags/ConfirmTitle=Apply tags",
		message,
		LOC "$$$/Photometoria/ApplyTags/ConfirmButton=Apply",
		LOC "$$$/Photometoria/ApplyTags/CancelButton=Cancel"
	)

	if confirm ~= 'ok' then
		return
	end

	local appliedCount = applyKeywords(matched)

	LrDialogs.message(
		LOC "$$$/Photometoria/ApplyTags/DoneTitle=Tags applied",
		LOC(
			"$$$/Photometoria/ApplyTags/DoneMsg=Tags applied successfully to ^1 photos.",
			tostring(appliedCount)
		),
		'info'
	)
end

return ApplyTagsDialog
