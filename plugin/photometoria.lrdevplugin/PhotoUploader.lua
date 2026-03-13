-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrDate = import 'LrDate'
local LrExportSession = import 'LrExportSession'
local LrFileUtils = import 'LrFileUtils'
local LrPathUtils = import 'LrPathUtils'
local LrProgressScope = import 'LrProgressScope'

local ServerConnection = require 'ServerConnection'

local PhotoUploader = {}

--- Creates a temporary directory for photo export.
local function createTempDir()
	local tempRoot = LrPathUtils.getStandardFilePath('temp')
	local dirName = 'photometoria_' .. tostring(math.floor(LrDate.currentTime()))
	local tempDir = LrPathUtils.child(tempRoot, dirName)
	LrFileUtils.createAllDirectories(tempDir)
	return tempDir
end

--- Deletes a file if it exists.
local function deleteFile(path)
	if LrFileUtils.exists(path) then
		LrFileUtils.delete(path)
	end
end

--- Deletes all files in a batch.
local function cleanupBatch(batch)
	for _, file in ipairs(batch) do
		deleteFile(file.filePath)
	end
end

--- Uploads a batch of exported files and tracks results.
--- Returns (uploadedCount, failedCount, networkOk).
local function uploadBatch(host, taskId, batch)
	local ok, result = ServerConnection.uploadPhotos(host, taskId, batch)

	if ok then
		local uploaded = #(result.uploaded or {})
		local failed = #(result.failed or {})
		return uploaded, failed, true
	end

	return 0, #batch, false
end

--- Exports photos and uploads them in micro-batches.
--- Returns { total, uploaded, failed, cancelled }.
function PhotoUploader.run(host, taskId, photos, batchSize)
	local totalPhotos = #photos

	local progressScope = LrProgressScope {
		title = LOC("$$$/Photometoria/Progress/Uploading=Uploading ^1 photos...", tostring(totalPhotos)),
	}

	local tempDir = createTempDir()

	local exportSession = LrExportSession {
		photosToExport = photos,
		exportSettings = {
			LR_format = 'JPEG',
			LR_jpeg_quality = 0.85,
			LR_export_destinationType = 'specificFolder',
			LR_export_destinationPathPrefix = tempDir,
			LR_export_useSubfolder = false,
			LR_size_doConstrain = true,
			LR_size_maxHeight = 2048,
			LR_size_maxWidth = 2048,
			LR_size_resizeType = 'wh',
			LR_size_units = 'pixels',
			LR_collisionHandling = 'rename',
			LR_reimportExportedPhoto = false,
		},
	}

	local batch = {}
	local processedCount = 0
	local totalUploaded = 0
	local totalFailed = 0
	local aborted = false

	for _, rendition in exportSession:renditions() do
		if progressScope:isCanceled() then
			break
		end

		local fileName = LrPathUtils.leafName(rendition.photo:getRawMetadata('path'))
		progressScope:setCaption(fileName)

		local success, pathOrMessage = rendition:waitForRender()
		processedCount = processedCount + 1
		progressScope:setPortionComplete(processedCount, totalPhotos)

		if success then
			batch[#batch + 1] = {
				clientId = rendition.photo:getRawMetadata('uuid'),
				filePath = pathOrMessage,
				fileName = LrPathUtils.leafName(pathOrMessage),
			}
		else
			totalFailed = totalFailed + 1
		end

		if #batch >= batchSize or (processedCount == totalPhotos and #batch > 0) then
			local uploaded, failed, networkOk = uploadBatch(host, taskId, batch)
			totalUploaded = totalUploaded + uploaded
			totalFailed = totalFailed + failed

			cleanupBatch(batch)
			batch = {}

			if not networkOk then
				aborted = true
				break
			end
		end
	end

	cleanupBatch(batch)
	deleteFile(tempDir)
	progressScope:done()

	return {
		total = totalPhotos,
		uploaded = totalUploaded,
		failed = totalFailed,
		cancelled = progressScope:isCanceled() or aborted,
	}
end

return PhotoUploader
