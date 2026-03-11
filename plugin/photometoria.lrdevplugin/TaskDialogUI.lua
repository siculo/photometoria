-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrColor = import 'LrColor'
local LrBinding = import 'LrBinding'
local LrPrefs = import 'LrPrefs'

local ServerConnection = require 'ServerConnection'
local TaskUtils = require 'TaskUtils'
local NewJobDialog = require 'NewJobDialog'
local MockData = require 'MockData'

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

--- Reusable progress bar component for LrView.
local ProgressBar = {}

local PB_FILLED = '\226\150\136'
local PB_EMPTY = ' '
local PB_WIDTH = 20

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
	if status == 'processing' or status == 'queued' then
		return '\226\159\179'
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

local currentJobs = {}
local jobsByTaskId = {}
local onJobSelected

--- Fetches jobs for all tasks and stores them in jobsByTaskId.
local function prefetchAllJobs(host, tasks)
	jobsByTaskId = {}
	for _, task in ipairs(tasks) do
		local ok, jobs = ServerConnection.listTaskJobs(host, task.task_id)
		jobsByTaskId[task.task_id] = ok and jobs or {}
	end
end

--- Clears the job detail panel.
local function clearJobDetail(props)
	props.jobDetailVisible = false
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

	props.contextText = ''
	props.contextSavedText = ''
	props.contextModified = false

	props.selectedJobValue = { 1 }
	props.jobListItems = {}

	props.jobDetailVisible = false
	props.jobProviderModel = ''
	props.jobStatusIndicator = ''
	props.jobProgressVisible = false
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

	props.taskSummary = string.format(
		'%d %s \194\183 %s \194\183 %d %s',
		photoCount,
		LOC "$$$/Photometoria/Task/Photos=photos",
		formatBytes(storageUsed),
		jobCount,
		LOC "$$$/Photometoria/Task/Jobs=job"
	)

	props.contextText = task.context or ''
	props.contextSavedText = task.context or ''
	props.contextModified = false

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

	if job.status == 'processing' or job.status == 'queued' then
		props.jobProgressVisible = true
		props.jobInfoText = formatJobRunningInfo(job)
		local processed = job.processed_photo_count or 0
		local total = job.photo_count or 0
		ProgressBar.set(props, 'jobDetail_pb', processed, total)
	else
		props.jobProgressVisible = false
		props.jobInfoText = formatJobCompletedInfo(job)
		ProgressBar.clear(props, 'jobDetail_pb')
	end

	props.btnApplyEnabled = (job.status == 'completed' or job.status == 'failed')
	props.btnRetryEnabled = (job.status == 'failed')
	props.btnRestartEnabled = (job.status == 'cancelled')
	props.btnCancelEnabled = (job.status == 'processing' or job.status == 'queued')
	props.btnRemoveEnabled = (job.status ~= 'processing' and job.status ~= 'queued')
end

--- Builds the task selector row: popup_menu + Delete button.
local function buildTaskSelectorRow(f, props)
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
				-- TODO: implement actual deletion, then clear lastActiveTaskId:
				-- local prefs = LrPrefs.prefsForPlugin()
				-- local task = tasks[props.selectedTask]
				-- if task and prefs.lastActiveTaskId == task.task_id then
				--     prefs.lastActiveTaskId = nil
				-- end
				LrDialogs.message(
					LOC "$$$/Photometoria/Mock/Delete=Delete Task",
					LOC "$$$/Photometoria/Mock/DeleteMsg=This would open the task deletion confirmation dialog.",
					'info'
				)
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
local function buildTaskSection(f, props)
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
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/ShowPhotos=Show Photos",
						LOC "$$$/Photometoria/Mock/ShowPhotosMsg=This would select the task photos in the Lightroom library.",
						'info'
					)
				end,
			},
		},

		f:static_text {
			title = LOC "$$$/Photometoria/Dialog/ContextLabel=Context",
			font = '<system/bold>',
			enabled = bind 'taskSelected',
		},

		f:edit_field {
			value = bind 'contextText',
			enabled = bind 'taskSelected',
			fill_horizontal = 1,
			height_in_lines = 4,
			immediate = true,
		},

		f:row {
			spacing = f:control_spacing(),

			f:push_button {
				enabled = bind 'contextModified',
				title = LOC "$$$/Photometoria/Button/SaveContext=Save",
				action = function()
					-- TODO: send context update to server, then update lastActiveTaskId
					props.contextSavedText = props.contextText
					props.contextModified = false
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/SaveContext=Save Context",
						LOC "$$$/Photometoria/Mock/SaveContextMsg=Context saved successfully.",
						'info'
					)
				end,
			},

			f:push_button {
				enabled = bind 'contextModified',
				title = LOC "$$$/Photometoria/Button/CancelContext=Cancel",
				action = function()
					props.contextText = props.contextSavedText
					props.contextModified = false
				end,
			},
		},
	}
end

--- Builds the job detail panel (right column of jobs section).
--- TODO: job mutation actions (start, retry, restart, cancel, remove) should
--- update prefs.lastActiveTaskId with the current task's task_id.
local function buildJobDetailPanel(f, props)
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

		f:column {
			visible = bind 'jobProgressVisible',
			fill_horizontal = 1,
			spacing = f:control_spacing(),

			ProgressBar.build(f, 'jobDetail_pb'),
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
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/ApplyTags=Applica tag",
						LOC "$$$/Photometoria/Mock/ApplyTagsMsg=This would open the apply tags confirmation dialog.",
						'info'
					)
				end,
			},

			f:push_button {
				enabled = bind 'btnRetryEnabled',
				title = '\226\154\160 ' .. LOC "$$$/Photometoria/Button/RetryFailed=Ritenta falliti",
				action = function()
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/RetryFailed=Ritenta falliti",
						LOC "$$$/Photometoria/Mock/RetryFailedMsg=This would open the retry failed confirmation dialog.",
						'info'
					)
				end,
			},

			f:push_button {
				enabled = bind 'btnRestartEnabled',
				title = '\226\159\179 ' .. LOC "$$$/Photometoria/Button/Restart=Riavvia",
				action = function()
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/Restart=Riavvia",
						LOC "$$$/Photometoria/Mock/RestartMsg=This would open the restart confirmation dialog.",
						'info'
					)
				end,
			},

			f:push_button {
				enabled = bind 'btnCancelEnabled',
				title = '\226\156\149 ' .. LOC "$$$/Photometoria/Button/CancelJob=Interrompi",
				action = function()
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/CancelJob=Interrompi",
						LOC "$$$/Photometoria/Mock/CancelJobMsg=This would open the job cancellation confirmation dialog.",
						'info'
					)
				end,
			},

			f:push_button {
				enabled = bind 'btnRemoveEnabled',
				title = '\240\159\151\145 ' .. LOC "$$$/Photometoria/Button/RemoveJob=Elimina",
				action = function()
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/RemoveJob=Elimina",
						LOC "$$$/Photometoria/Mock/RemoveJobMsg=This would open the job removal confirmation dialog.",
						'info'
					)
				end,
			},
		},
	}
end

--- Builds the jobs section with master-detail layout.
local function buildJobsSection(f, props, tasks)
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
						local selection = NewJobDialog.showDialog(
							MockData.providers,
							task.photo_count or 0
						)
						if not selection then
							return
						end
						onTaskSelected(props, tasks, props.selectedTask)
					end,
				},
			},

			buildJobDetailPanel(f, props),
		},
	}
end

--- Builds the complete dialog contents.
local function buildContents(f, props, tasks)
	return f:column {
		bind_to_object = props,
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		buildTaskSelectorRow(f, props),

		f:static_text {
			visible = bind 'noTasksVisible',
			title = LOC "$$$/Photometoria/Task/NoTasks=No tasks available. Create a task by adding photos via the plugin menu.",
			fill_horizontal = 1,
		},

		buildTaskSection(f, props),

		buildJobsSection(f, props, tasks),
	}
end

--- Shows the task management dialog. Must be called from within an async task.
--- @param host string Server host:port
--- @param tasks table Array of TaskSummary from the server
function TaskDialogUI.showDialog(host, tasks)
	prefetchAllJobs(host, tasks)

	LrFunctionContext.callWithContext('TaskDialog', function(context)
		local f = LrView.osFactory()

		local props = LrBinding.makePropertyTable(context)
		initProperties(props, tasks)

		props:addObserver('selectedTask', function(propTable, key, value)
			if value then
				onTaskSelected(propTable, tasks, value)
			end
		end)

		props:addObserver('selectedJobValue', function(propTable)
			onJobSelected(propTable)
		end)

		props:addObserver('contextText', function(propTable, key, value)
			propTable.contextModified = (value ~= propTable.contextSavedText)
		end)

		if props.selectedTask then
			onTaskSelected(props, tasks, props.selectedTask)
		end

		LrDialogs.presentModalDialog {
			title = LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
			contents = buildContents(f, props, tasks),
			actionVerb = LOC "$$$/Photometoria/Button/Close=Close",
			cancelVerb = '< exclude >',
		}
	end)
end

return TaskDialogUI
