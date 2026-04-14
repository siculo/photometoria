-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrApplication = import 'LrApplication'
local LrBinding = import 'LrBinding'
local LrDialogs = import 'LrDialogs'
local LrFunctionContext = import 'LrFunctionContext'
local LrView = import 'LrView'

local ServerConnection = require 'ServerConnection'

local ApplyTagsDialog = {}

local bind = LrView.bind

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
--- When removeExisting is true, existing keywords are removed before adding new ones.
--- Both removal and addition share a single undo step.
--- Returns the number of photos that received keywords.
local function applyKeywords(matched, removeExisting)
	local catalog = LrApplication.activeCatalog()
	local appliedCount = 0

	catalog:withWriteAccessDo(
		LOC "$$$/Photometoria/Undo/ApplyTags=Photometoria: Apply tags",
		function()
			for _, entry in ipairs(matched) do
				if removeExisting then
					local existing = entry.photo:getRawMetadata('keywords')
					if existing then
						for _, kw in ipairs(existing) do
							entry.photo:removeKeyword(kw)
						end
					end
				end
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

--- Returns an array of summary lines describing what will be applied.
local function buildConfirmLines(matched, failedCount, missingCount)
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

	return lines
end

--- Counts the total number of existing keywords across all matched photos.
local function countExistingKeywords(matched)
	local total = 0
	for _, entry in ipairs(matched) do
		local existing = entry.photo:getRawMetadata('keywords')
		if existing then
			total = total + #existing
		end
	end
	return total
end

--- Builds the modal dialog contents for the apply-tags confirmation.
local function buildConfirmContents(f, props, lines, existingKeywordCount, photoCount)
	local contents = {
		bind_to_object = props,
		spacing = f:control_spacing(),
		fill_horizontal = 1,
		width = 400,
	}

	for _, line in ipairs(lines) do
		contents[#contents + 1] = f:static_text {
			title = line,
			fill_horizontal = 1,
		}
	end

	contents[#contents + 1] = f:separator { fill_horizontal = 1 }

	contents[#contents + 1] = f:checkbox {
		title = LOC "$$$/Photometoria/ApplyTags/RemoveExisting=Remove existing keywords before applying",
		value = bind 'removeExisting',
	}

	contents[#contents + 1] = f:static_text {
		title = LOC(
			"$$$/Photometoria/ApplyTags/ExistingKeywordsWarning=^1 existing keywords will be removed from ^2 photos.",
			tostring(existingKeywordCount),
			tostring(photoCount)
		),
		font = '<system/small>',
		enabled = bind 'removeExisting',
		fill_horizontal = 1,
	}

	return f:column(contents)
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

	local lines = buildConfirmLines(matched, failedCount, missingCount)
	local existingKeywordCount = countExistingKeywords(matched)

	local confirmed = false
	local removeExisting = false

	LrFunctionContext.callWithContext('ApplyTagsDialog', function(context)
		local f = LrView.osFactory()
		local props = LrBinding.makePropertyTable(context)
		props.removeExisting = false

		local result = LrDialogs.presentModalDialog {
			title = LOC "$$$/Photometoria/ApplyTags/ConfirmTitle=Apply tags",
			contents = buildConfirmContents(f, props, lines, existingKeywordCount, #matched),
			actionVerb = LOC "$$$/Photometoria/ApplyTags/ConfirmButton=Apply",
		}

		if result == 'ok' then
			confirmed = true
			removeExisting = props.removeExisting
		end
	end)

	if not confirmed then
		return
	end

	local appliedCount = applyKeywords(matched, removeExisting)

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
