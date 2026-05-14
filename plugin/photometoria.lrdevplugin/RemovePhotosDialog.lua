-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrBinding = import 'LrBinding'
local LrPrefs = import 'LrPrefs'
local LrProgressScope = import 'LrProgressScope'
local LrTasks = import 'LrTasks'
local LrApplication = import 'LrApplication'

local CatalogIdentity = require 'CatalogIdentity'
local ServerConnection = require 'ServerConnection'
local TaskUtils = require 'TaskUtils'
local Guard = require 'Guard'
local RecentHosts = require 'RecentHosts'

local bind = LrView.bind

local DIALOG_TITLE = LOC "$$$/Photometoria/RemovePhotos/Title=Remove Photos"

--- Builds popup_menu items from a task list.
local function buildTaskPopupItems(tasks)
	local items = {}
	for i, task in ipairs(tasks) do
		items[#items + 1] = { title = task.name, value = i }
	end
	return items
end

--- Returns photos to remove (task photos whose client_id is in selectedUUIDs)
--- and the count of selected photos not present in the task.
local function computeRemovable(taskIndex, tasks, photosByTaskId, selectedUUIDs, totalCount)
	local task = tasks[taskIndex]
	if not task then return {}, totalCount end

	local taskPhotos = photosByTaskId[task.task_id] or {}
	local removable = {}
	for _, tp in ipairs(taskPhotos) do
		if selectedUUIDs[tp.client_id] then
			removable[#removable + 1] = tp
		end
	end
	return removable, totalCount - #removable
end

--- Updates summary props and confirmEnabled based on the currently selected task.
local function updateSummary(props, tasks, photosByTaskId, selectedUUIDs, totalCount)
	local removable, notRemovable = computeRemovable(
		props.selectedTask, tasks, photosByTaskId, selectedUUIDs, totalCount
	)

	props.confirmEnabled = (#removable > 0)

	if #removable > 0 then
		props.removableSummary = LOC(
			"$$$/Photometoria/RemovePhotos/WillRemove=^1 photos will be removed",
			tostring(#removable)
		)
	else
		props.removableSummary = LOC "$$$/Photometoria/RemovePhotos/NoneToRemove=No selected photos are in this task."
	end

	if notRemovable > 0 then
		props.notRemovableSummary = LOC(
			"$$$/Photometoria/RemovePhotos/NotInTask=^1 not in this task",
			tostring(notRemovable)
		)
	else
		props.notRemovableSummary = ''
	end
end

--- Builds the selected-photos summary section.
local function buildPhotoSection(f, totalCount)
	return f:group_box {
		title = LOC "$$$/Photometoria/RemovePhotos/PhotoSectionTitle=Selected photos",
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:static_text {
			title = LOC("$$$/Photometoria/RemovePhotos/PhotoCount=^1 photos selected", tostring(totalCount)),
			fill_horizontal = 1,
		},
	}
end

--- Builds the task selection and live summary section.
local function buildTaskSection(f)
	return f:group_box {
		title = LOC "$$$/Photometoria/RemovePhotos/TaskSectionTitle=Remove from",
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:popup_menu {
			value = bind 'selectedTask',
			items = bind 'taskItems',
			width = 250,
		},

		f:static_text {
			title = bind 'removableSummary',
			fill_horizontal = 1,
			font = '<system/small>',
		},

		f:static_text {
			title = bind 'notRemovableSummary',
			fill_horizontal = 1,
			font = '<system/small>',
		},
	}
end

--- Builds the complete dialog view.
local function buildContents(f, props, totalCount)
	return f:column {
		bind_to_object = props,
		spacing = f:control_spacing(),
		fill_horizontal = 1,
		width = 400,

		buildPhotoSection(f, totalCount),
		buildTaskSection(f),
	}
end

LrTasks.startAsyncTask(function()
	if not Guard.acquire('RemovePhotosDialog') then
		LrDialogs.message(
			DIALOG_TITLE,
			LOC "$$$/Photometoria/RemovePhotos/AlreadyRunning=Remove Photos is already in progress.",
			'info'
		)
		return
	end

	local prefs = LrPrefs.prefsForPlugin()
	local host = RecentHosts.extractHost(prefs.serverHost or '')

	if not ServerConnection.isValidHostPort(host) then
		LrDialogs.message(
			DIALOG_TITLE,
			LOC "$$$/Photometoria/Error/NoServer=Server not configured. Please set the server address in Plugin Manager.",
			'critical'
		)
		Guard.release('RemovePhotosDialog')
		return
	end

	local catalog = LrApplication.activeCatalog()

	if not catalog:getTargetPhoto() then
		LrDialogs.message(
			DIALOG_TITLE,
			LOC "$$$/Photometoria/RemovePhotos/NoPhotos=No photos are selected in Lightroom.",
			'warning'
		)
		Guard.release('RemovePhotosDialog')
		return
	end

	local selectedPhotos = catalog:getTargetPhotos()

	local selectedUUIDs = {}
	for _, photo in ipairs(selectedPhotos) do
		local uuid = photo:getRawMetadata('uuid')
		if uuid then
			selectedUUIDs[uuid] = true
		end
	end
	local totalCount = #selectedPhotos

	local catalogId = CatalogIdentity.catalogId()

	local taskScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/LoadingTasks=Loading tasks...",
	}
	local taskSuccess, tasks = ServerConnection.listTasks(host, catalogId)
	taskScope:done()

	if not taskSuccess then
		LrDialogs.message(DIALOG_TITLE, tasks.message, 'critical')
		Guard.release('RemovePhotosDialog')
		return
	end

	if #tasks == 0 then
		LrDialogs.message(
			DIALOG_TITLE,
			LOC "$$$/Photometoria/RemovePhotos/NoTasks=No tasks available on the server.",
			'info'
		)
		Guard.release('RemovePhotosDialog')
		return
	end

	local prefetchScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/LoadingTaskPhotos=Loading task photos...",
	}
	local photosByTaskId = {}
	for _, task in ipairs(tasks) do
		local ok, data = ServerConnection.listTaskPhotos(host, task.task_id)
		photosByTaskId[task.task_id] = ok and (data.photos or {}) or {}
	end
	prefetchScope:done()

	local confirmedTaskId = nil
	local photosToRemove = nil

	LrFunctionContext.callWithContext('RemovePhotosDialog', function(context)
		local f = LrView.osFactory()
		local props = LrBinding.makePropertyTable(context)

		local defaultIndex = TaskUtils.findTaskIndex(tasks, prefs.lastActiveTaskId) or 1
		props.taskItems = buildTaskPopupItems(tasks)
		props.selectedTask = defaultIndex
		props.removableSummary = ''
		props.notRemovableSummary = ''
		props.confirmEnabled = false

		updateSummary(props, tasks, photosByTaskId, selectedUUIDs, totalCount)

		props:addObserver('selectedTask', function(propTable)
			updateSummary(propTable, tasks, photosByTaskId, selectedUUIDs, totalCount)
		end)

		local result = LrDialogs.presentModalDialog {
			title = DIALOG_TITLE,
			contents = buildContents(f, props, totalCount),
			actionVerb = LOC "$$$/Photometoria/Button/ConfirmRemove=Remove",
			actionBinding = {
				enabled = {
					bind_to_object = props,
					key = 'confirmEnabled',
				},
			},
		}

		if result == 'ok' then
			local task = tasks[props.selectedTask]
			if task then
				confirmedTaskId = task.task_id
				photosToRemove = computeRemovable(
					props.selectedTask, tasks, photosByTaskId, selectedUUIDs, totalCount
				)
				prefs.lastActiveTaskId = task.task_id
			end
		end
	end)

	Guard.release('RemovePhotosDialog')

	if not confirmedTaskId or not photosToRemove or #photosToRemove == 0 then
		return
	end

	local checkScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/CheckingJobs=Checking job status...",
	}
	local jobsOk, jobs = ServerConnection.listTaskJobs(host, confirmedTaskId)
	checkScope:done()

	if jobsOk then
		for _, job in ipairs(jobs) do
			if job.status == 'processing' or job.status == 'queued' then
				LrDialogs.message(
					DIALOG_TITLE,
					LOC "$$$/Photometoria/RemovePhotos/ActiveJob=Cannot remove photos: this task has active jobs. Wait for all jobs to complete before removing photos.",
					'warning'
				)
				return
			end
		end
	end

	local deleteScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/RemovingPhotos=Removing photos...",
	}
	local succeeded = 0
	local failed = 0
	for _, photo in ipairs(photosToRemove) do
		local ok = ServerConnection.deletePhoto(host, photo.photo_id)
		if ok then
			succeeded = succeeded + 1
		else
			failed = failed + 1
		end
	end
	deleteScope:done()

	if failed == 0 then
		LrDialogs.message(
			DIALOG_TITLE,
			LOC("$$$/Photometoria/RemovePhotos/Success=^1 photos removed successfully.", tostring(succeeded)),
			'info'
		)
	else
		LrDialogs.message(
			DIALOG_TITLE,
			LOC("$$$/Photometoria/RemovePhotos/Partial=^1 of ^2 photos removed (^3 failed).",
				tostring(succeeded), tostring(#photosToRemove), tostring(failed)),
			'warning'
		)
	end
end)
