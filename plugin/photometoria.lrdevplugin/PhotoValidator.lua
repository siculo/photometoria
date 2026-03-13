-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local PhotoValidator = {}

--- Validates target photos from the catalog for upload eligibility.
--- Excludes missing files and videos. Must be called from an async task context.
--- When progressScope is provided, reports validation progress (0.0–1.0).
--- Returns { total, uploadable, missing, videos, photos }.
function PhotoValidator.validate(catalog, progressScope)
	local LrFileUtils = import 'LrFileUtils'
	local targetPhotos = catalog:getTargetPhotos()
	local total = #targetPhotos

	local result = {
		total = total,
		uploadable = 0,
		missing = 0,
		videos = 0,
		photos = {},
	}

	for i, photo in ipairs(targetPhotos) do
		if progressScope and progressScope:isCanceled() then
			return nil
		end

		if photo:getRawMetadata('isVideo') then
			result.videos = result.videos + 1
		else
			local path = photo:getRawMetadata('path')
			if LrFileUtils.exists(path) then
				result.uploadable = result.uploadable + 1
				result.photos[#result.photos + 1] = photo
			else
				result.missing = result.missing + 1
			end
		end

		if progressScope then
			progressScope:setPortionComplete(i, total)
		end
	end

	return result
end

--- Formats a validation result into a human-readable summary string.
--- Uses progressive detail: shows exclusion reasons only when present.
function PhotoValidator.formatSummary(result)
	if result.total == 0 then
		return LOC "$$$/Photometoria/AddPhotos/SummaryNoPhotos=No target photos"
	end

	local photosStr = LOC("$$$/Photometoria/AddPhotos/SummaryPhotos=^1 photos", tostring(result.total))

	if result.missing == 0 and result.videos == 0 then
		return photosStr
	end

	local reasons = {}
	if result.missing > 0 then
		reasons[#reasons + 1] = LOC("$$$/Photometoria/AddPhotos/SummaryMissing=^1 missing", tostring(result.missing))
	end
	if result.videos > 0 then
		reasons[#reasons + 1] = LOC("$$$/Photometoria/AddPhotos/SummaryVideos=^1 video", tostring(result.videos))
	end

	local uploadableStr = LOC("$$$/Photometoria/AddPhotos/SummaryUploadable=^1 uploadable",
		tostring(result.uploadable))

	return string.format('%s \194\183 %s (%s)',
		photosStr, uploadableStr, table.concat(reasons, ', '))
end

return PhotoValidator
