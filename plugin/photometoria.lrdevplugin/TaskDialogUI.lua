-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrApplication = import 'LrApplication'
local LrApplicationView = import 'LrApplicationView'
local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrColor = import 'LrColor'
local LrBinding = import 'LrBinding'
local LrPrefs = import 'LrPrefs'
local LrTasks = import 'LrTasks'
local LrDate = import 'LrDate'

local ServerConnection = require 'ServerConnection'
local TaskUtils = require 'TaskUtils'
local NewJobDialog = require 'NewJobDialog'
local ApplyTagsDialog = require 'ApplyTagsDialog'

local TaskDialogUI = {}

local bind = LrView.bind

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

--- Formats an ISO 8601 date string as a locale-friendly date and time.
local function formatDateTime(isoString)
	if not isoString or isoString == '' then
		return ''
	end
	local y, m, d, h, min, s = isoString:match('(%d+)-(%d+)-(%d+)T(%d+):(%d+):(%d+)')
	if not y then
		return ''
	end
	local time = LrDate.timeFromComponents(
		tonumber(y), tonumber(m), tonumber(d),
		tonumber(h), tonumber(min), tonumber(s), 'UTC'
	)
	return LrDate.formatMediumDate(time) .. ', ' .. LrDate.formatShortTime(time)
end

--- Reusable progress bar component for LrView.
local ProgressBar = {}

local PB_FILLED = '\226\150\136'
local PB_EMPTY = '_'
local PB_WIDTH = 50

--- Initializes the internal properties for a progress bar.
function ProgressBar.init(props, key)
	props[key .. '_bar'] = ''
	props[key .. '_pct'] = ''
end

--- Builds the view tree: a disabled edit_field (bar) + static_text (percentage).
function ProgressBar.build(f, key)
	return f:row {
		spacing = f:label_spacing(),

		f:edit_field {
			value = bind(key .. '_bar'),
			font = { name = 'Courier New', size = 11 },
			enabled = false,
			fill_horizontal = 1,
		},

		f:static_text {
			title = bind(key .. '_pct'),
			width = 35,
			alignment = 'right',
		},
	}
end

--- Updates the progress bar value.
function ProgressBar.set(props, key, processed, total)
	if total <= 0 then
		props[key .. '_bar'] = ''
		props[key .. '_pct'] = ''
		return
	end
	local ratio = processed / total
	local filled = math.floor(ratio * PB_WIDTH + 0.5)
	local empty = PB_WIDTH - filled
	props[key .. '_bar'] = string.rep(PB_FILLED, filled) .. string.rep(PB_EMPTY, empty)
	props[key .. '_pct'] = string.format('%d%%', math.floor(ratio * 100))
end

--- Clears the progress bar.
function ProgressBar.clear(props, key)
	props[key .. '_bar'] = ''
	props[key .. '_pct'] = ''
end

--- Returns a text status icon for a job status.
local function jobStatusIcon(status)
	if status == 'processing' then
		return '\226\159\179'
	elseif status == 'queued' then
		return '\226\143\179'
	elseif status == 'completed' then
		return '\226\156\147'
	elseif status == 'failed' then
		return '\226\154\160'
	elseif status == 'cancelled' then
		return '\226\156\149'
	end
	return ''
end

--- Returns a localized label for a job status.
local function jobStatusLabel(status)
	if status == 'processing' then
		return LOC "$$$/Photometoria/JobStatus/Running=In progress"
	elseif status == 'queued' then
		return LOC "$$$/Photometoria/JobStatus/Queued=Queued"
	elseif status == 'completed' then
		return LOC "$$$/Photometoria/JobStatus/Completed=Completed"
	elseif status == 'failed' then
		return LOC "$$$/Photometoria/JobStatus/Errored=With errors"
	elseif status == 'cancelled' then
		return LOC "$$$/Photometoria/JobStatus/Cancelled=Cancelled"
	end
	return status
end

--- Builds popup_menu items from the task list.
local function buildTaskPopupItems(tasks)
	local items = {}
	for i, task in ipairs(tasks) do
		local jobInfo = ''
		local jobCount = task.job_count or 0
		if jobCount > 0 then
			jobInfo = string.format('  (%d %s)', jobCount, LOC "$$$/Photometoria/Task/Jobs=job")
		end
		items[#items + 1] = {
			title = task.name .. jobInfo,
			value = i,
		}
	end
	return items
end

--- Builds simple_list items from a job list.
local function buildJobListItems(jobs)
	local items = {}
	if not jobs then
		return items
	end
	for i, job in ipairs(jobs) do
		items[#items + 1] = {
			title = job.model .. '  ' .. jobStatusIcon(job.status),
			value = i,
		}
	end
	return items
end

--- Formats a job info string for processing/queued jobs.
local function formatJobRunningInfo(job)
	local processed = job.processed_photo_count or 0
	local total = job.photo_count or 0
	return string.format(
		'%d/%d %s',
		processed, total,
		LOC "$$$/Photometoria/Job/Photos=foto processate"
	)
end

--- Formats a job info string for completed/failed/cancelled jobs.
local function formatJobCompletedInfo(job)
	local processed = job.processed_photo_count or 0
	local total = job.photo_count or 0
	local failed = total - processed
	local text = string.format(
		'%d/%d %s',
		processed, total,
		LOC "$$$/Photometoria/Job/Photos=foto processate"
	)
	if job.status == 'failed' and failed > 0 then
		text = text .. ' \226\128\148 ' .. LOC(
			"$$$/Photometoria/Job/Failed=^1 fallite",
			failed
		)
	end
	return text
end

local SPINNER_FRAMES = {
	'\226\160\139',
	'\226\160\153',
	'\226\160\185',
	'\226\160\184',
	'\226\160\188',
	'\226\160\180',
	'\226\160\166',
	'\226\160\167',
	'\226\160\135',
	'\226\160\143',
}
local SPINNER_INTERVAL = 0.4

local currentJobs = {}
local jobsByTaskId = {}
local providers = {}
local defaultProviderName = nil
local dialogOpen = false
local spinnerActive = false
local onJobSelected

--- Fetches jobs for all tasks and stores them in jobsByTaskId.
local function prefetchAllJobs(host, tasks)
	jobsByTaskId = {}
	for _, task in ipairs(tasks) do
		local ok, jobs = ServerConnection.listTaskJobs(host, task.task_id)
		jobsByTaskId[task.task_id] = ok and jobs or {}
	end
end

--- Fetches provider list and model details, stores in module-level tables.
local function prefetchProviders(host)
	providers = {}
	defaultProviderName = nil
	local ok, data = ServerConnection.listProviders(host)
	if not ok then
		return
	end
	defaultProviderName = data.default
	for _, entry in ipairs(data.providers or {}) do
		local detailOk, detail = ServerConnection.providerDetails(host, entry.name)
		if detailOk then
			providers[#providers + 1] = detail
		end
	end
end

--- Clears the job detail panel.
local function clearJobDetail(props)
	props.jobDetailVisible = false
	props.jobSpinnerVisible = false
	props.jobWaitingVisible = false
	spinnerActive = false
	props.btnApplyEnabled = false
	props.btnRetryEnabled = false
	props.btnRestartEnabled = false
	props.btnCancelEnabled = false
	props.btnRemoveEnabled = false
end

--- Initializes all bindable properties.
local function initProperties(props, tasks)
	local prefs = LrPrefs.prefsForPlugin()
	props.selectedTask = TaskUtils.findTaskIndex(tasks, prefs.lastActiveTaskId) or (#tasks > 0 and 1 or nil)
	props.taskPopupItems = buildTaskPopupItems(tasks)

	props.taskSelected = (#tasks > 0)
	props.noTasksVisible = (#tasks == 0)

	props.taskSummary = ''
	props.deleteEnabled = (#tasks > 0)
	props.deleteDisabledReason = ''
	props.deleteDisabledVisible = false

	props.nameText = ''
	props.nameSavedText = ''
	props.contextText = ''
	props.contextSavedText = ''
	props.taskModified = false

	props.selectedJobValue = { 1 }
	props.jobListItems = {}

	props.jobDetailVisible = false
	props.jobProviderModel = ''
	props.jobStatusIndicator = ''
	props.jobProgressVisible = false
	props.jobSpinnerText = ''
	props.jobSpinnerVisible = false
	props.jobWaitingVisible = false
	props.jobInfoText = ''
	ProgressBar.init(props, 'jobDetail_pb')

	props.btnApplyEnabled = false
	props.btnRetryEnabled = false
	props.btnRestartEnabled = false
	props.btnCancelEnabled = false
	props.btnRemoveEnabled = false
end

--- Updates the detail panel when a task is selected.
--- Reads jobs from the prefetched jobsByTaskId table.
local function onTaskSelected(props, tasks, index)
	local task = tasks[index]
	if not task then
		return
	end

	local photoCount = task.photo_count or 0
	local storageUsed = task.storage_used or 0
	local jobCount = task.job_count or 0

	local createdAt = formatDateTime(task.created_at)

	props.taskSummary = string.format(
		'%d %s \194\183 %s \194\183 %d %s \194\183 %s %s',
		photoCount,
		LOC "$$$/Photometoria/Task/Photos=photos",
		formatBytes(storageUsed),
		jobCount,
		LOC "$$$/Photometoria/Task/Jobs=job",
		LOC "$$$/Photometoria/Task/CreatedOn=created on",
		createdAt
	)

	props.nameText = task.name or ''
	props.nameSavedText = task.name or ''
	props.contextText = task.context or ''
	props.contextSavedText = task.context or ''
	props.taskModified = false

	local jobs = jobsByTaskId[task.task_id] or {}
	currentJobs = jobs

	local hasActiveJobs = false
	for _, job in ipairs(jobs) do
		if job.status == 'processing' or job.status == 'queued' then
			hasActiveJobs = true
			break
		end
	end

	if hasActiveJobs then
		props.deleteEnabled = false
		props.deleteDisabledVisible = true
		props.deleteDisabledReason = LOC "$$$/Photometoria/Task/CannotDelete=Task with active job \226\128\148 cannot delete"
	else
		props.deleteEnabled = true
		props.deleteDisabledVisible = false
		props.deleteDisabledReason = ''
	end

	props.jobListItems = buildJobListItems(jobs)
	props.selectedJobValue = (#jobs > 0) and { 1 } or {}

	onJobSelected(props)
end

--- Updates job detail panel when a job is selected.
onJobSelected = function(props)
	local selectedJob = props.selectedJobValue
	local jobIndex = selectedJob and selectedJob[1]
	local job = jobIndex and currentJobs[jobIndex]
	if not job then
		clearJobDetail(props)
		return
	end

	props.jobDetailVisible = true
	props.jobProviderModel = job.model
	props.jobStatusIndicator = '[' .. jobStatusIcon(job.status) .. ' ' .. jobStatusLabel(job.status) .. ']'

	if job.status == 'processing' then
		props.jobProgressVisible = true
		props.jobSpinnerVisible = true
		props.jobWaitingVisible = false
		spinnerActive = true
		props.jobInfoText = formatJobRunningInfo(job)
		local processed = job.processed_photo_count or 0
		local total = job.photo_count or 0
		ProgressBar.set(props, 'jobDetail_pb', processed, total)
	elseif job.status == 'queued' then
		props.jobProgressVisible = true
		props.jobSpinnerVisible = false
		props.jobWaitingVisible = true
		spinnerActive = false
		props.jobInfoText = formatJobRunningInfo(job)
		local processed = job.processed_photo_count or 0
		local total = job.photo_count or 0
		ProgressBar.set(props, 'jobDetail_pb', processed, total)
	else
		props.jobProgressVisible = false
		props.jobSpinnerVisible = false
		props.jobWaitingVisible = false
		spinnerActive = false
		props.jobInfoText = formatJobCompletedInfo(job)
		ProgressBar.clear(props, 'jobDetail_pb')
	end

	props.btnApplyEnabled = (job.status == 'completed' or job.status == 'failed' or job.status == 'cancelled')
	props.btnRetryEnabled = (job.status == 'failed')
	props.btnRestartEnabled = (job.status == 'cancelled')
	props.btnCancelEnabled = (job.status == 'processing' or job.status == 'queued')
	props.btnRemoveEnabled = (job.status ~= 'processing' and job.status ~= 'queued')
end

--- Refreshes job-related UI for the current task without touching name/context.
--- Used by the polling loop and after job creation.
local function refreshJobsUI(props, tasks)
	local taskIndex = props.selectedTask
	local task = tasks[taskIndex]
	if not task then
		return
	end

	local jobs = jobsByTaskId[task.task_id] or {}
	currentJobs = jobs

	props.jobListItems = buildJobListItems(jobs)

	local selectedIdx = props.selectedJobValue and props.selectedJobValue[1]
	if not selectedIdx or selectedIdx > #jobs then
		props.selectedJobValue = (#jobs > 0) and { 1 } or {}
	end

	onJobSelected(props)

	local hasActiveJobs = false
	for _, job in ipairs(jobs) do
		if job.status == 'processing' or job.status == 'queued' then
			hasActiveJobs = true
			break
		end
	end

	if hasActiveJobs then
		props.deleteEnabled = false
		props.deleteDisabledVisible = true
		props.deleteDisabledReason = LOC "$$$/Photometoria/Task/CannotDelete=Task with active job \226\128\148 cannot delete"
	else
		props.deleteEnabled = true
		props.deleteDisabledVisible = false
		props.deleteDisabledReason = ''
	end
end

local POLL_INTERVAL_SECONDS = 3

--- Starts an async polling loop that updates job data for the selected task.
--- Runs until dialogOpen becomes false.
local function startJobPolling(host, tasks, props)
	LrTasks.startAsyncTask(function()
		while dialogOpen do
			LrTasks.sleep(POLL_INTERVAL_SECONDS)
			if not dialogOpen then
				break
			end

			local taskIndex = props.selectedTask
			local task = tasks[taskIndex]
			if not task then
				break
			end

			local hasActive = false
			local jobs = jobsByTaskId[task.task_id] or {}
			for _, job in ipairs(jobs) do
				if job.status == 'processing' or job.status == 'queued' then
					hasActive = true
					break
				end
			end

			if hasActive then
				local polledTaskId = task.task_id
				local ok, newJobs = ServerConnection.listTaskJobs(host, polledTaskId)
				if ok then
					jobsByTaskId[polledTaskId] = newJobs
					local currentTask = tasks[props.selectedTask]
					if currentTask and currentTask.task_id == polledTaskId then
						refreshJobsUI(props, tasks)
					end
				end
			end
		end
	end)

	LrTasks.startAsyncTask(function()
		local frame = 1
		while dialogOpen do
			LrTasks.sleep(SPINNER_INTERVAL)
			if not dialogOpen then
				break
			end
			if spinnerActive then
				props.jobSpinnerText = SPINNER_FRAMES[frame]
				frame = (frame % #SPINNER_FRAMES) + 1
			end
		end
	end)
end

--- Builds the task selector row: popup_menu + Delete button.
local function buildTaskSelectorRow(f, props, host, tasks)
	return f:row {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:popup_menu {
			value = bind 'selectedTask',
			items = bind 'taskPopupItems',
			enabled = bind 'taskSelected',
			width = 280,
		},

		f:push_button {
			title = LOC "$$$/Photometoria/Button/Delete=Delete",
			enabled = bind 'deleteEnabled',
			action = function()
				local task = tasks[props.selectedTask]
				if not task then
					return
				end

				local confirmed = LrDialogs.confirm(
					LOC "$$$/Photometoria/Confirm/DeleteTaskTitle=Delete task",
					LOC("$$$/Photometoria/Confirm/DeleteTaskMsg=Delete task '^1' and all its photos? This action cannot be undone.", task.name)
				)
				if confirmed ~= 'ok' then
					return
				end

				LrTasks.startAsyncTask(function()
					local ok, data = ServerConnection.deleteTask(host, task.task_id)
					if ok then
						local prefs = LrPrefs.prefsForPlugin()
						if prefs.lastActiveTaskId == task.task_id then
							prefs.lastActiveTaskId = nil
						end

						jobsByTaskId[task.task_id] = nil
						table.remove(tasks, props.selectedTask)

						props.taskPopupItems = buildTaskPopupItems(tasks)
						props.taskSelected = (#tasks > 0)
						props.noTasksVisible = (#tasks == 0)

						if #tasks > 0 then
							props.selectedTask = 1
						else
							props.selectedTask = nil
							props.deleteEnabled = false
							clearJobDetail(props)
							props.taskSummary = ''
							props.nameText = ''
							props.contextText = ''
							props.jobListItems = {}
						end
					else
						LrDialogs.message(
							LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
							data.message,
							'critical'
						)
					end
				end)
			end,
		},

		f:static_text {
			visible = bind 'deleteDisabledVisible',
			title = bind 'deleteDisabledReason',
			text_color = LrColor(0.85, 0.2, 0.2),
			font = '<system/small>',
			fill_horizontal = 1,
		},
	}
end

--- Builds the task section: summary, context, and action buttons.
local function buildTaskSection(f, props, host, tasks)
	return f:group_box {
		title = LOC "$$$/Photometoria/Dialog/TaskTitle=Task detail",
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:row {
			spacing = f:control_spacing(),
			fill_horizontal = 1,

			f:static_text {
				title = bind 'taskSummary',
			},

			f:push_button {
				title = LOC "$$$/Photometoria/Button/ShowPhotos=Show Photos in Library",
				enabled = bind 'taskSelected',
				place_horizontal = 1,
				action = function()
					local task = tasks[props.selectedTask]
					if not task then return end
					local currentHost = host

					LrTasks.startAsyncTask(function()
						local ok, data = ServerConnection.listTaskPhotos(currentHost, task.task_id)
						if not ok then
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								data.message,
								'critical'
							)
							return
						end

						if data.count == 0 then
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								LOC "$$$/Photometoria/ShowPhotos/NoPhotos=This task has no photos.",
								'info'
							)
							return
						end

						local catalog = LrApplication.activeCatalog()
						local foundPhotos = {}
						local missingCount = 0

						for _, summary in ipairs(data.photos) do
							if summary.client_id then
								local photo = catalog:findPhotoByUuid(summary.client_id)
								if photo then
									foundPhotos[#foundPhotos + 1] = photo
								else
									missingCount = missingCount + 1
								end
							else
								missingCount = missingCount + 1
							end
						end

						if #foundPhotos == 0 then
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								LOC "$$$/Photometoria/ShowPhotos/NoneFound=None of the task photos were found in this catalog.",
								'warning'
							)
							return
						end

						catalog:withWriteAccessDo(
							LOC "$$$/Photometoria/Undo/ShowPhotos=Photometoria: Show Photos",
							function()
								local collection = catalog:createCollection(
									'Photometoria',
									nil,
									true
								)
								collection:removeAllPhotos()
								collection:addPhotos(foundPhotos)
							end
						)

						local collection = catalog:createCollection(
							'Photometoria',
							nil,
							true
						)
						catalog:setActiveSources({ collection })
						LrApplicationView.switchToModule('library')

						if missingCount > 0 then
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								LOC("$$$/Photometoria/ShowPhotos/SomeMissing=^1 of ^2 photos were not found in this catalog.",
									tostring(missingCount), tostring(data.count)),
								'warning'
							)
						end
					end)
				end,
			},
		},

		f:row {
			spacing = f:label_spacing(),

			f:static_text {
				title = LOC "$$$/Photometoria/Dialog/NameLabel=Name",
				alignment = 'right',
				width = 80,
				enabled = bind 'taskSelected',
			},

			f:edit_field {
				value = bind 'nameText',
				enabled = bind 'taskSelected',
				fill_horizontal = 1,
				immediate = true,
			},
		},

		f:row {
			spacing = f:label_spacing(),

			f:static_text {
				title = LOC "$$$/Photometoria/Dialog/ContextLabel=Context",
				alignment = 'right',
				width = 80,
				enabled = bind 'taskSelected',
			},

			f:edit_field {
				value = bind 'contextText',
				enabled = bind 'taskSelected',
				fill_horizontal = 1,
				height_in_lines = 4,
				immediate = true,
			},
		},

		f:row {
			spacing = f:control_spacing(),

			f:push_button {
				enabled = bind 'taskModified',
				title = LOC "$$$/Photometoria/Button/SaveTask=Save",
				action = function()
					local task = tasks[props.selectedTask]
					if not task then
						return
					end

					local trimmedName = props.nameText:match('^%s*(.-)%s*$')
					if trimmedName == '' then
						return
					end

					local contextText = props.contextText:match('^%s*(.-)%s*$')
					if contextText == '' then
						return
					end

					LrTasks.startAsyncTask(function()
						local ok, data = ServerConnection.updateTask(host, task.task_id, trimmedName, contextText)

						if ok then
							task.name = trimmedName
							task.context = contextText
							props.nameText = trimmedName
							props.nameSavedText = trimmedName
							props.contextText = contextText
							props.contextSavedText = contextText
							props.taskModified = false
							props.taskPopupItems = buildTaskPopupItems(tasks)
							local prefs = LrPrefs.prefsForPlugin()
							prefs.lastActiveTaskId = task.task_id
						else
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								data.message,
								'critical'
							)
						end
					end)
				end,
			},

			f:push_button {
				enabled = bind 'taskModified',
				title = LOC "$$$/Photometoria/Button/CancelEdit=Cancel",
				action = function()
					props.nameText = props.nameSavedText
					props.contextText = props.contextSavedText
					props.taskModified = false
				end,
			},
		},
	}
end

--- Builds the job detail panel (right column of jobs section).
--- TODO: job mutation actions (start, retry, restart, cancel, remove) should
--- update prefs.lastActiveTaskId with the current task's task_id.
local function buildJobDetailPanel(f, props, host, tasks)
	return f:column {
		visible = bind 'jobDetailVisible',
		fill_horizontal = 1,
		spacing = f:control_spacing(),

		f:row {
			spacing = f:control_spacing(),
			fill_horizontal = 1,

			f:static_text {
				title = bind 'jobProviderModel',
				font = '<system/bold>',
				fill_horizontal = 1,
			},

			f:static_text {
				title = bind 'jobStatusIndicator',
				width_in_chars = 18,
				alignment = 'right',
			},
		},

		f:static_text {
			title = LOC "$$$/Photometoria/Job/Waiting=Waiting...",
			visible = bind 'jobWaitingVisible',
			fill_horizontal = 1,
			font = '<system/small>',
			enabled = false,
		},

		f:row {
			spacing = f:label_spacing(),

			f:static_text {
				title = bind('jobDetail_pb_bar'),
				visible = bind 'jobProgressVisible',
				font = { name = 'Courier New', size = 11 },
				text_color = LrColor(0.3, 0.3, 0.3),
				fill_horizontal = 1,
			},

			f:static_text {
				title = bind('jobDetail_pb_pct'),
				visible = bind 'jobProgressVisible',
				width = 35,
				alignment = 'right',
			},

			f:static_text {
				title = bind 'jobSpinnerText',
				visible = bind 'jobSpinnerVisible',
				width_in_chars = 2,
			},
		},

		f:static_text {
			title = bind 'jobInfoText',
			fill_horizontal = 1,
		},

		f:spacer { height = 8 },

		f:row {
			spacing = f:control_spacing(),

			f:push_button {
				enabled = bind 'btnApplyEnabled',
				title = '\226\156\147 ' .. LOC "$$$/Photometoria/Button/ApplyTags=Applica tag",
				action = function()
					local jobIndex = props.selectedJobValue and props.selectedJobValue[1]
					local job = jobIndex and currentJobs[jobIndex]
					if not job then
						return
					end

					LrTasks.startAsyncTask(function()
						ApplyTagsDialog.run(host, job.job_id)
					end)
				end,
			},

			f:push_button {
				enabled = bind 'btnRetryEnabled',
				title = '\226\154\160 ' .. LOC "$$$/Photometoria/Button/RetryFailed=Ritenta falliti",
				action = function()
					local task = tasks[props.selectedTask]
					local jobIndex = props.selectedJobValue and props.selectedJobValue[1]
					local job = jobIndex and currentJobs[jobIndex]
					if not task or not job then
						return
					end

					local confirmed = LrDialogs.confirm(
						LOC "$$$/Photometoria/Confirm/RetryJobTitle=Retry failed photos",
						LOC("$$$/Photometoria/Confirm/RetryJobMsg=Create a new job to retry failed photos from ^1?", job.model)
					)
					if confirmed ~= 'ok' then
						return
					end

					LrTasks.startAsyncTask(function()
						local ok, data = ServerConnection.retryJob(host, job.job_id)
						if ok then
							local newJobId = data.job_id
							local jOk, jobs = ServerConnection.listTaskJobs(host, task.task_id)
							if jOk then
								jobsByTaskId[task.task_id] = jobs
								task.job_count = #jobs
							end
							props.taskPopupItems = buildTaskPopupItems(tasks)
							refreshJobsUI(props, tasks)

							if newJobId then
								local freshJobs = jobsByTaskId[task.task_id] or {}
								for i, j in ipairs(freshJobs) do
									if j.job_id == newJobId then
										props.selectedJobValue = { i }
										break
									end
								end
							end
						else
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								data.message,
								'critical'
							)
						end
					end)
				end,
			},

			f:push_button {
				enabled = bind 'btnRestartEnabled',
				title = '\226\159\179 ' .. LOC "$$$/Photometoria/Button/Restart=Riavvia",
				action = function()
					local task = tasks[props.selectedTask]
					local jobIndex = props.selectedJobValue and props.selectedJobValue[1]
					local job = jobIndex and currentJobs[jobIndex]
					if not task or not job then
						return
					end

					local confirmed = LrDialogs.confirm(
						LOC "$$$/Photometoria/Confirm/RestartJobTitle=Restart job",
						LOC("$$$/Photometoria/Confirm/RestartJobMsg=Create a new job to reprocess unfinished photos from ^1?", job.model)
					)
					if confirmed ~= 'ok' then
						return
					end

					LrTasks.startAsyncTask(function()
						local ok, data = ServerConnection.retryJob(host, job.job_id)
						if ok then
							local newJobId = data.job_id
							local jOk, jobs = ServerConnection.listTaskJobs(host, task.task_id)
							if jOk then
								jobsByTaskId[task.task_id] = jobs
								task.job_count = #jobs
							end
							props.taskPopupItems = buildTaskPopupItems(tasks)
							refreshJobsUI(props, tasks)

							if newJobId then
								local freshJobs = jobsByTaskId[task.task_id] or {}
								for i, j in ipairs(freshJobs) do
									if j.job_id == newJobId then
										props.selectedJobValue = { i }
										break
									end
								end
							end
						else
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								data.message,
								'critical'
							)
						end
					end)
				end,
			},

			f:push_button {
				enabled = bind 'btnCancelEnabled',
				title = '\226\156\149 ' .. LOC "$$$/Photometoria/Button/CancelJob=Interrompi",
				action = function()
					local task = tasks[props.selectedTask]
					local jobIndex = props.selectedJobValue and props.selectedJobValue[1]
					local job = jobIndex and currentJobs[jobIndex]
					if not task or not job then
						return
					end

					local confirmed = LrDialogs.confirm(
						LOC "$$$/Photometoria/Confirm/CancelJobTitle=Cancel job",
						LOC("$$$/Photometoria/Confirm/CancelJobMsg=Cancel job ^1? Photos not yet processed will be skipped.", job.model)
					)
					if confirmed ~= 'ok' then
						return
					end

					LrTasks.startAsyncTask(function()
						local ok, data = ServerConnection.cancelJob(host, job.job_id)
						if ok then
							local jOk, jobs = ServerConnection.listTaskJobs(host, task.task_id)
							if jOk then
								jobsByTaskId[task.task_id] = jobs
							end
							refreshJobsUI(props, tasks)
						else
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								data.message,
								'critical'
							)
						end
					end)
				end,
			},

			f:push_button {
				enabled = bind 'btnRemoveEnabled',
				title = '\240\159\151\145 ' .. LOC "$$$/Photometoria/Button/RemoveJob=Elimina",
				action = function()
					local task = tasks[props.selectedTask]
					local jobIndex = props.selectedJobValue and props.selectedJobValue[1]
					local job = jobIndex and currentJobs[jobIndex]
					if not task or not job then
						return
					end

					local confirmed = LrDialogs.confirm(
						LOC "$$$/Photometoria/Confirm/RemoveJobTitle=Delete job",
						LOC("$$$/Photometoria/Confirm/RemoveJobMsg=Delete job ^1? This action cannot be undone.", job.model)
					)
					if confirmed ~= 'ok' then
						return
					end

					LrTasks.startAsyncTask(function()
						local ok, data = ServerConnection.deleteJob(host, job.job_id)
						if ok then
							local jOk, jobs = ServerConnection.listTaskJobs(host, task.task_id)
							if jOk then
								jobsByTaskId[task.task_id] = jobs
								task.job_count = #jobs
							end
							props.taskPopupItems = buildTaskPopupItems(tasks)
							refreshJobsUI(props, tasks)
						else
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								data.message,
								'critical'
							)
						end
					end)
				end,
			},
		},
	}
end

--- Builds the jobs section with master-detail layout.
local function buildJobsSection(f, props, host, tasks)
	return f:group_box {
		title = LOC "$$$/Photometoria/Dialog/JobsTitle=Task jobs",
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:row {
			spacing = f:dialog_spacing(),
			fill_horizontal = 1,

			f:column {
				width = 240,
				spacing = f:control_spacing(),

				f:simple_list {
					value = bind 'selectedJobValue',
					items = bind 'jobListItems',
					enabled = bind 'taskSelected',
					width = 240,
					height = 120,
				},

				f:push_button {
					title = LOC "$$$/Photometoria/Button/NewJob=+ Start New Job",
					enabled = bind 'taskSelected',
					fill_horizontal = 1,
					action = function()
						local task = tasks[props.selectedTask]
						if not task then
							return
						end

						if #providers == 0 then
							LrDialogs.message(
								LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
								LOC "$$$/Photometoria/Error/NoProviders=No AI providers configured on the server.",
								'warning'
							)
							return
						end

						local selection = NewJobDialog.showDialog(
							providers,
							task.photo_count or 0,
							defaultProviderName
						)
						if not selection then
							return
						end

						LrTasks.startAsyncTask(function()
							local ok, data = ServerConnection.createJob(host, task.task_id, selection.model, selection.language)
							if ok then
								local newJobId = data.job_id
								local jOk, jobs = ServerConnection.listTaskJobs(host, task.task_id)
								if jOk then
									jobsByTaskId[task.task_id] = jobs
									task.job_count = #jobs
								end
								props.taskPopupItems = buildTaskPopupItems(tasks)
								onTaskSelected(props, tasks, props.selectedTask)

								if newJobId then
									local freshJobs = jobsByTaskId[task.task_id] or {}
									for i, job in ipairs(freshJobs) do
										if job.job_id == newJobId then
											props.selectedJobValue = { i }
											break
										end
									end
								end
							else
								LrDialogs.message(
									LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
									data.message,
									'critical'
								)
							end
						end)
					end,
				},
			},

			buildJobDetailPanel(f, props, host, tasks),
		},
	}
end

--- Builds the complete dialog contents.
local function buildContents(f, props, host, tasks)
	return f:column {
		bind_to_object = props,
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		buildTaskSelectorRow(f, props, host, tasks),

		f:static_text {
			visible = bind 'noTasksVisible',
			title = LOC "$$$/Photometoria/Task/NoTasks=No tasks available. Create a task by adding photos via the plugin menu.",
			fill_horizontal = 1,
		},

		buildTaskSection(f, props, host, tasks),

		buildJobsSection(f, props, host, tasks),
	}
end

--- Shows the task management dialog. Must be called from within an async task.
--- @param host string Server host:port
--- @param tasks table Array of TaskSummary from the server
function TaskDialogUI.showDialog(host, tasks)
	prefetchAllJobs(host, tasks)
	prefetchProviders(host)

	LrFunctionContext.callWithContext('TaskDialog', function(context)
		local f = LrView.osFactory()

		local props = LrBinding.makePropertyTable(context)
		initProperties(props, tasks)

		props:addObserver('selectedTask', function(propTable, key, value)
			if not value then
				return
			end
			onTaskSelected(propTable, tasks, value)
			local task = tasks[value]
			if task then
				LrTasks.startAsyncTask(function()
					local ok, jobs = ServerConnection.listTaskJobs(host, task.task_id)
					if ok then
						jobsByTaskId[task.task_id] = jobs
						local currentTask = tasks[propTable.selectedTask]
						if currentTask and currentTask.task_id == task.task_id then
							refreshJobsUI(propTable, tasks)
						end
					end
				end)
			end
		end)

		props:addObserver('selectedJobValue', function(propTable)
			onJobSelected(propTable)
		end)

		props:addObserver('nameText', function(propTable, key, value)
			propTable.taskModified = (value ~= propTable.nameSavedText)
				or (propTable.contextText ~= propTable.contextSavedText)
		end)

		props:addObserver('contextText', function(propTable, key, value)
			propTable.taskModified = (value ~= propTable.contextSavedText)
				or (propTable.nameText ~= propTable.nameSavedText)
		end)

		if props.selectedTask then
			onTaskSelected(props, tasks, props.selectedTask)
		end

		dialogOpen = true
		startJobPolling(host, tasks, props)

		LrDialogs.presentModalDialog {
			title = LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
			contents = buildContents(f, props, host, tasks),
			actionVerb = LOC "$$$/Photometoria/Button/Close=Close",
			cancelVerb = '< exclude >',
		}

		dialogOpen = false
	end)
end

return TaskDialogUI
