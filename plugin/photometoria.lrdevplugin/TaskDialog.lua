-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrColor = import 'LrColor'
local LrBinding = import 'LrBinding'
local LrPrefs = import 'LrPrefs'
local LrProgressScope = import 'LrProgressScope'
local LrTasks = import 'LrTasks'

local ServerConnection = require 'ServerConnection'
local MockData = require 'MockData'

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
	if status == 'running' then
		return '\226\159\179'
	elseif status == 'completed' then
		return '\226\156\147'
	elseif status == 'errored' then
		return '\226\154\160'
	elseif status == 'cancelled' then
		return '\226\156\149'
	end
	return ''
end

--- Returns a localized label for a job status.
local function jobStatusLabel(status)
	if status == 'running' then
		return LOC "$$$/Photometoria/JobStatus/Running=In progress"
	elseif status == 'completed' then
		return LOC "$$$/Photometoria/JobStatus/Completed=Completed"
	elseif status == 'errored' then
		return LOC "$$$/Photometoria/JobStatus/Errored=With errors"
	elseif status == 'cancelled' then
		return LOC "$$$/Photometoria/JobStatus/Cancelled=Cancelled"
	end
	return status
end

--- Builds status icon summary for a task's jobs.
local function taskJobIcons(task)
	local hasRunning, hasCompleted, hasErrored, hasCancelled = false, false, false, false
	for _, job in ipairs(task.jobs) do
		if job.status == 'running' then hasRunning = true
		elseif job.status == 'completed' then hasCompleted = true
		elseif job.status == 'errored' then hasErrored = true
		elseif job.status == 'cancelled' then hasCancelled = true
		end
	end
	local icons = {}
	if hasCompleted then icons[#icons + 1] = '\226\156\147' end
	if hasErrored then icons[#icons + 1] = '\226\154\160' end
	if hasCancelled then icons[#icons + 1] = '\226\156\149' end
	if hasRunning then icons[#icons + 1] = '\226\159\179' end
	if #icons == 0 then
		return ''
	end
	return '  (' .. table.concat(icons, ' ') .. ')'
end

--- Builds popup_menu items from the task list.
local function buildTaskPopupItems(tasks)
	local items = {}
	for i, task in ipairs(tasks) do
		items[#items + 1] = {
			title = task.name .. taskJobIcons(task),
			value = i,
		}
	end
	return items
end

--- Builds simple_list items from a task's job list.
local function buildJobListItems(task)
	local items = {}
	if not task or not task.jobs then
		return items
	end
	for i, job in ipairs(task.jobs) do
		items[#items + 1] = {
			title = job.provider .. ' \194\183 ' .. job.model .. '  ' .. jobStatusIcon(job.status),
			value = i,
		}
	end
	return items
end

--- Formats a job info string for running jobs.
local function formatJobRunningInfo(job)
	local text = string.format(
		'%d/%d %s',
		job.photosProcessed, job.photosTotal,
		LOC "$$$/Photometoria/Job/Photos=foto processate"
	)
	if job.estimatedRemaining ~= '' then
		text = text .. ' \226\128\148 ' .. LOC(
			"$$$/Photometoria/Job/Remaining=~^1. rimanenti",
			job.estimatedRemaining
		)
	end
	return text
end

--- Formats a job info string for completed/errored/cancelled jobs.
local function formatJobCompletedInfo(job)
	local text = string.format(
		'%d/%d %s',
		job.photosProcessed, job.photosTotal,
		LOC "$$$/Photometoria/Job/Photos=foto processate"
	)
	if job.status == 'errored' and job.errorCount > 0 then
		text = text .. ' \226\128\148 ' .. LOC(
			"$$$/Photometoria/Job/Failed=^1 fallite",
			job.errorCount
		)
	end
	if job.duration ~= '' then
		text = text .. ' \226\128\148 ' .. LOC(
			"$$$/Photometoria/Job/CompletedIn=completato in ^1.",
			job.duration
		)
	end
	return text
end

local onJobSelected

--- Initializes all bindable properties.
local function initProperties(props, tasks)
	props.selectedTask = 1
	props.taskPopupItems = buildTaskPopupItems(tasks)

	props.taskSummary = ''
	props.deleteEnabled = true
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
local function onTaskSelected(props, tasks, index)
	local task = tasks[index]
	if not task then
		return
	end

	props.taskSummary = string.format(
		'%d %s \194\183 %s \194\183 %d %s',
		task.photoCount,
		LOC "$$$/Photometoria/Task/Photos=photos",
		formatBytes(task.sizeBytes),
		#task.jobs,
		LOC "$$$/Photometoria/Task/Jobs=job"
	)

	local hasActiveJobs = false
	for _, job in ipairs(task.jobs) do
		if job.status == 'running' then
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

	props.contextText = task.context or ''
	props.contextSavedText = task.context or ''
	props.contextModified = false

	props.jobListItems = buildJobListItems(task)
	props.selectedJobValue = (#task.jobs > 0) and { 1 } or {}

	onJobSelected(props, tasks)
end

--- Updates job detail panel when a job is selected.
onJobSelected = function(props, tasks)
	local task = tasks[props.selectedTask]
	if not task then
		props.jobDetailVisible = false
		return
	end

	local selectedJob = props.selectedJobValue
	local jobIndex = selectedJob and selectedJob[1]
	local job = jobIndex and task.jobs[jobIndex]
	if not job then
		props.jobDetailVisible = false
		props.btnApplyEnabled = false
		props.btnRetryEnabled = false
		props.btnRestartEnabled = false
		props.btnCancelEnabled = false
		props.btnRemoveEnabled = false
		return
	end

	props.jobDetailVisible = true
	props.jobProviderModel = job.provider .. ' \194\183 ' .. job.model
	props.jobStatusIndicator = '[' .. jobStatusIcon(job.status) .. ' ' .. jobStatusLabel(job.status) .. ']'

	if job.status == 'running' then
		props.jobProgressVisible = true
		props.jobInfoText = formatJobRunningInfo(job)
		ProgressBar.set(props, 'jobDetail_pb', job.photosProcessed, job.photosTotal)
	else
		props.jobProgressVisible = false
		props.jobInfoText = formatJobCompletedInfo(job)
		ProgressBar.clear(props, 'jobDetail_pb')
	end

	props.btnApplyEnabled = (job.status == 'completed' or job.status == 'errored')
	props.btnRetryEnabled = (job.status == 'errored' and job.errorCount > 0)
	props.btnRestartEnabled = (job.status == 'cancelled')
	props.btnCancelEnabled = (job.status == 'running')
	props.btnRemoveEnabled = (job.status ~= 'running')
end

--- Builds the task selector row: popup_menu + Delete button.
local function buildTaskSelectorRow(f, props)
	return f:row {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:popup_menu {
			value = bind 'selectedTask',
			items = bind 'taskPopupItems',
			width = 280,
		},

		f:push_button {
			title = LOC "$$$/Photometoria/Button/Delete=Delete",
			enabled = bind 'deleteEnabled',
			action = function()
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
		},
	}
end

--- Builds the context section.
local function buildContextSection(f, props)
	return f:column {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:static_text {
			title = LOC "$$$/Photometoria/Dialog/ContextTitle=Context",
			font = '<system/bold>',
		},

		f:edit_field {
			value = bind 'contextText',
			fill_horizontal = 1,
			height_in_lines = 4,
			immediate = true,
		},

		f:row {
			visible = bind 'contextModified',
			spacing = f:control_spacing(),

			f:push_button {
				title = LOC "$$$/Photometoria/Button/SaveContext=Save",
				action = function()
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
				title = LOC "$$$/Photometoria/Button/CancelContext=Cancel",
				action = function()
					props.contextText = props.contextSavedText
					props.contextModified = false
				end,
			},
		},

		f:separator { fill_horizontal = 1 },
	}
end

--- Builds the task info row.
local function buildTaskInfoRow(f, props)
	return f:row {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:static_text {
			title = bind 'taskSummary',
		},

		f:push_button {
			title = LOC "$$$/Photometoria/Button/ShowPhotos=Show Photos in Library",
			place_horizontal = 1,
			action = function()
				LrDialogs.message(
					LOC "$$$/Photometoria/Mock/ShowPhotos=Show Photos",
					LOC "$$$/Photometoria/Mock/ShowPhotosMsg=This would select the task photos in the Lightroom library.",
					'info'
				)
			end,
		},
	}
end

--- Builds the job detail panel (right column of jobs section).
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
local function buildJobsSection(f, props)
	return f:column {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:static_text {
			title = LOC "$$$/Photometoria/Dialog/JobsTitle=Jobs",
			font = '<system/bold>',
		},

		f:row {
			spacing = f:dialog_spacing(),
			fill_horizontal = 1,

			f:column {
				width = 240,
				spacing = f:control_spacing(),

				f:simple_list {
					value = bind 'selectedJobValue',
					items = bind 'jobListItems',
					width = 240,
					height = 120,
				},

				f:push_button {
					title = LOC "$$$/Photometoria/Button/NewJob=+ Start New Job",
					fill_horizontal = 1,
					action = function()
						LrDialogs.message(
							LOC "$$$/Photometoria/Mock/NewJob=New Job",
							LOC "$$$/Photometoria/Mock/NewJobMsg=This would open the create job dialog.",
							'info'
						)
					end,
				},
			},

			buildJobDetailPanel(f, props),
		},
	}
end

--- Builds the complete dialog contents.
local function buildContents(f, props)
	return f:column {
		bind_to_object = props,
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		buildTaskSelectorRow(f, props),

		f:separator { fill_horizontal = 1 },

		buildContextSection(f, props),

		buildTaskInfoRow(f, props),

		f:separator { fill_horizontal = 1 },

		buildJobsSection(f, props),
	}
end

LrTasks.startAsyncTask(function()
	local prefs = LrPrefs.prefsForPlugin()
	local host = prefs.serverHost or ''

	if not ServerConnection.isValidHostPort(host) then
		LrDialogs.message(
			LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
			LOC "$$$/Photometoria/Error/NoServer=Server not configured. Please set the server address in Plugin Manager.",
			'critical'
		)
		return
	end

	local progressScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/Connecting=Connecting to server...",
	}

	local success, data = ServerConnection.fetch(host)
	progressScope:done()

	if not success then
		LrDialogs.message(
			LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
			data.message,
			'critical'
		)
		return
	end

	LrFunctionContext.callWithContext('TaskDialog', function(context)
		local f = LrView.osFactory()
		local tasks = MockData.tasks

		local props = LrBinding.makePropertyTable(context)
		initProperties(props, tasks)

		props:addObserver('selectedTask', function(propTable, key, value)
			if value then
				onTaskSelected(propTable, tasks, value)
			end
		end)

		props:addObserver('selectedJobValue', function(propTable)
			onJobSelected(propTable, tasks)
		end)

		props:addObserver('contextText', function(propTable, key, value)
			propTable.contextModified = (value ~= propTable.contextSavedText)
		end)

		onTaskSelected(props, tasks, 1)

		LrDialogs.presentModalDialog {
			title = LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
			contents = buildContents(f, props),
			actionVerb = LOC "$$$/Photometoria/Button/Close=Close",
			cancelVerb = '< exclude >',
		}
	end)
end)
