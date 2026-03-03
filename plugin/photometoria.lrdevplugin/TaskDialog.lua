-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrColor = import 'LrColor'
local LrBinding = import 'LrBinding'

local MockData = require 'MockData'

local bind = LrView.bind
local MAX_JOB_SLOTS = 5

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
--- Usage: ProgressBar.init(props, key), ProgressBar.build(f, key),
--- ProgressBar.set(props, key, processed, total).
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

--- Returns a localized label for a task status.
local function taskStatusLabel(status)
	if status == 'active' then
		return LOC "$$$/Photometoria/TaskStatus/Active=Active"
	elseif status == 'completed' then
		return LOC "$$$/Photometoria/TaskStatus/Completed=Completed"
	elseif status == 'errors' then
		return LOC "$$$/Photometoria/TaskStatus/Errors=Errors"
	end
	return status
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

--- Builds simple_list items from the task list.
local function buildTaskListItems(tasks)
	local items = {}
	for i, task in ipairs(tasks) do
		items[#items + 1] = {
			title = task.name .. '  [' .. taskStatusLabel(task.status) .. ']',
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
			title = job.provider .. ' \194\183 ' .. job.model .. '  [' .. jobStatusLabel(job.status) .. ']',
			value = i,
		}
	end
	return items
end

--- Formats a job progress string.
local function formatJobProgress(job)
	if job.status == 'running' then
		local pct = math.floor((job.photosProcessed / job.photosTotal) * 100)
		local text = string.format('%d/%d (%d%%)', job.photosProcessed, job.photosTotal, pct)
		if job.estimatedRemaining ~= '' then
			text = text .. ' — ' .. LOC("$$$/Photometoria/Job/Remaining=~^1 remaining", job.estimatedRemaining)
		end
		return text
	elseif job.status == 'completed' then
		return string.format('%d/%d — %s', job.photosProcessed, job.photosTotal, job.duration)
	elseif job.status == 'errored' then
		return string.format(
			'%d/%d — %s — %d %s',
			job.photosProcessed, job.photosTotal, job.duration,
			job.errorCount, LOC "$$$/Photometoria/Job/Errors=errors"
		)
	elseif job.status == 'cancelled' then
		return string.format('%d/%d — %s', job.photosProcessed, job.photosTotal, job.duration)
	end
	return ''
end

local onJobSelected

--- Initializes all bindable properties.
local function initProperties(props, tasks)
	props.selectedTaskValue = { 1 }
	props.taskListItems = buildTaskListItems(tasks)

	props.taskSummary = ''
	props.taskStatusText = ''
	props.deleteEnabled = true
	props.deleteDisabledReason = ''

	props.contextText = ''
	props.contextSavedText = ''
	props.contextModified = false

	props.selectedJobValue = { 1 }
	props.jobListItems = {}

	for i = 1, MAX_JOB_SLOTS do
		props['jobSlot' .. i .. '_visible'] = false
		props['jobSlot' .. i .. '_providerModel'] = ''
		props['jobSlot' .. i .. '_status'] = ''
		props['jobSlot' .. i .. '_statusIndicator'] = ''
		props['jobSlot' .. i .. '_progress'] = ''
		ProgressBar.init(props, 'jobSlot' .. i .. '_pb')
	end

	props.btnRemoveEnabled = false
	props.btnRetryEnabled = false
	props.btnApplyEnabled = false
	props.btnCancelJobEnabled = false
	props.btnNewJobEnabled = true
end

--- Updates the detail panel when a task is selected.
local function onTaskSelected(props, tasks, index)
	local task = tasks[index]
	if not task then
		return
	end

	props.taskSummary = string.format(
		'%d %s — %s',
		task.photoCount,
		LOC "$$$/Photometoria/Task/Photos=photos",
		formatBytes(task.sizeBytes)
	)
	props.taskStatusText = '[' .. taskStatusLabel(task.status) .. ']'

	local hasActiveJobs = false
	for _, job in ipairs(task.jobs) do
		if job.status == 'running' then
			hasActiveJobs = true
			break
		end
	end

	if hasActiveJobs then
		props.deleteEnabled = false
		props.deleteDisabledReason = LOC "$$$/Photometoria/Task/CannotDelete=Cannot delete: task has active jobs"
	else
		props.deleteEnabled = true
		props.deleteDisabledReason = ''
	end

	props.contextText = task.context or ''
	props.contextSavedText = task.context or ''
	props.contextModified = false

	props.jobListItems = buildJobListItems(task)
	props.selectedJobValue = (#task.jobs > 0) and { 1 } or {}

	local hasCompleted = false
	for i = 1, MAX_JOB_SLOTS do
		local job = task.jobs[i]
		if job then
			props['jobSlot' .. i .. '_visible'] = true
			props['jobSlot' .. i .. '_providerModel'] = job.provider .. ' \194\183 ' .. job.model
			props['jobSlot' .. i .. '_status'] = jobStatusLabel(job.status)
			props['jobSlot' .. i .. '_statusIndicator'] = '[' .. jobStatusLabel(job.status) .. ']'
			props['jobSlot' .. i .. '_progress'] = formatJobProgress(job)
			ProgressBar.set(props, 'jobSlot' .. i .. '_pb', job.photosProcessed, job.photosTotal)
			if job.status == 'completed' or job.status == 'errored' or job.status == 'cancelled' then
				hasCompleted = true
			end
		else
			props['jobSlot' .. i .. '_visible'] = false
			props['jobSlot' .. i .. '_providerModel'] = ''
			props['jobSlot' .. i .. '_status'] = ''
			props['jobSlot' .. i .. '_statusIndicator'] = ''
			props['jobSlot' .. i .. '_progress'] = ''
			ProgressBar.clear(props, 'jobSlot' .. i .. '_pb')
		end
	end

	props.btnRemoveEnabled = hasCompleted

	onJobSelected(props, tasks)
end

--- Updates button states when a job is selected.
onJobSelected = function(props, tasks)
	local selectedTask = props.selectedTaskValue
	local taskIndex = selectedTask and selectedTask[1]
	local task = taskIndex and tasks[taskIndex]
	if not task then
		return
	end

	local selectedJob = props.selectedJobValue
	local jobIndex = selectedJob and selectedJob[1]
	local job = jobIndex and task.jobs[jobIndex]
	if not job then
		props.btnRetryEnabled = false
		props.btnApplyEnabled = false
		props.btnCancelJobEnabled = false
		return
	end

	props.btnRetryEnabled = (job.status == 'errored' and job.errorCount > 0)
	props.btnApplyEnabled = (job.status == 'completed')
	props.btnCancelJobEnabled = (job.status == 'running')
end

--- Builds a single pre-allocated job slot view.
local function buildJobSlot(f, props, slotIndex)
	local prefix = 'jobSlot' .. slotIndex .. '_'
	return f:column {
		visible = bind(prefix .. 'visible'),
		fill_horizontal = 1,
		margin_top = 4,

		f:row {
			spacing = f:control_spacing(),

			f:static_text {
				title = bind(prefix .. 'providerModel'),
				font = '<system/bold>',
			},

			f:static_text {
				title = bind(prefix .. 'statusIndicator'),
			},
		},

		ProgressBar.build(f, prefix .. 'pb'),

		f:static_text {
			title = bind(prefix .. 'progress'),
			fill_horizontal = 1,
		},

		f:separator { fill_horizontal = 1 },
	}
end

--- Builds the complete dialog contents.
local function buildContents(f, props)
	return f:row {
		bind_to_object = props,
		spacing = f:dialog_spacing(),

		f:column {
			width = 280,
			spacing = f:control_spacing(),

			f:static_text {
				title = LOC "$$$/Photometoria/Dialog/TasksTitle=Tasks",
				font = '<system/bold>',
			},

			f:simple_list {
				value = bind 'selectedTaskValue',
				items = bind 'taskListItems',
				width = 270,
				height = 100,
			},

			f:spacer { height = 4 },

			f:static_text {
				title = bind 'taskSummary',
				fill_horizontal = 1,
			},

			f:static_text {
				title = bind 'taskStatusText',
				font = '<system/bold>',
			},

			f:spacer { height = 8 },
			f:separator { fill_horizontal = 1 },
			f:spacer { height = 8 },

			f:push_button {
				title = LOC "$$$/Photometoria/Button/ShowPhotos=Show Photos in Library",
				fill_horizontal = 1,
				action = function()
					LrDialogs.message(
						LOC "$$$/Photometoria/Mock/ShowPhotos=Show Photos",
						LOC "$$$/Photometoria/Mock/ShowPhotosMsg=This would select the task photos in the Lightroom library.",
						'info'
					)
				end,
			},

			f:push_button {
				title = LOC "$$$/Photometoria/Button/Delete=Delete",
				fill_horizontal = 1,
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
				title = bind 'deleteDisabledReason',
				text_color = LrColor(0.85, 0.55, 0.0),
				fill_horizontal = 1,
			},
		},

		f:column {
			fill_horizontal = 1,
			spacing = f:control_spacing(),

			f:row {
				spacing = f:dialog_spacing(),

				f:column {
					width = 300,
					spacing = f:control_spacing(),

					f:static_text {
						title = LOC "$$$/Photometoria/Dialog/ContextTitle=Context",
						font = '<system/bold>',
					},

					f:edit_field {
						value = bind 'contextText',
						width_in_chars = 35,
						height_in_lines = 10,
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
				},

				f:column {
					fill_horizontal = 1,
					spacing = f:control_spacing(),

					f:static_text {
						title = LOC "$$$/Photometoria/Dialog/JobsTitle=Jobs",
						font = '<system/bold>',
					},

					f:simple_list {
						value = bind 'selectedJobValue',
						items = bind 'jobListItems',
						width = 280,
						height = 80,
					},

					f:spacer { height = 4 },

					buildJobSlot(f, props, 1),
					buildJobSlot(f, props, 2),
					buildJobSlot(f, props, 3),
					buildJobSlot(f, props, 4),
					buildJobSlot(f, props, 5),

					f:spacer { height = 8 },

					f:row {
						spacing = f:control_spacing(),

						f:push_button {
							title = LOC "$$$/Photometoria/Button/RemoveCompleted=Remove Completed",
							enabled = bind 'btnRemoveEnabled',
							action = function()
								LrDialogs.message(
									LOC "$$$/Photometoria/Mock/RemoveCompleted=Remove Completed",
									LOC "$$$/Photometoria/Mock/RemoveCompletedMsg=This would remove all completed jobs from the list.",
									'info'
								)
							end,
						},

						f:push_button {
							title = LOC "$$$/Photometoria/Button/RetryFailed=Retry Failed",
							enabled = bind 'btnRetryEnabled',
							action = function()
								LrDialogs.message(
									LOC "$$$/Photometoria/Mock/RetryFailed=Retry Failed",
									LOC "$$$/Photometoria/Mock/RetryFailedMsg=This would open the retry failed confirmation dialog.",
									'info'
								)
							end,
						},

						f:push_button {
							title = LOC "$$$/Photometoria/Button/ApplyTags=Apply Tags",
							enabled = bind 'btnApplyEnabled',
							action = function()
								LrDialogs.message(
									LOC "$$$/Photometoria/Mock/ApplyTags=Apply Tags",
									LOC "$$$/Photometoria/Mock/ApplyTagsMsg=This would open the apply tags confirmation dialog.",
									'info'
								)
							end,
						},
					},

					f:row {
						spacing = f:control_spacing(),

						f:push_button {
							title = LOC "$$$/Photometoria/Button/CancelJob=Cancel Job",
							enabled = bind 'btnCancelJobEnabled',
							action = function()
								LrDialogs.message(
									LOC "$$$/Photometoria/Mock/CancelJob=Cancel Job",
									LOC "$$$/Photometoria/Mock/CancelJobMsg=This would open the job cancellation confirmation dialog.",
									'info'
								)
							end,
						},

						f:push_button {
							title = LOC "$$$/Photometoria/Button/NewJob=+ New Job",
							enabled = bind 'btnNewJobEnabled',
							action = function()
								LrDialogs.message(
									LOC "$$$/Photometoria/Mock/NewJob=New Job",
									LOC "$$$/Photometoria/Mock/NewJobMsg=This would open the create job dialog.",
									'info'
								)
							end,
						},
					},
				},
			},
		},
	}
end

LrFunctionContext.callWithContext('TaskDialog', function(context)
	local f = LrView.osFactory()
	local tasks = MockData.tasks

	local props = LrBinding.makePropertyTable(context)
	initProperties(props, tasks)

	props:addObserver('selectedTaskValue', function(propTable, key, value)
		local index = value and value[1]
		if index then
			onTaskSelected(propTable, tasks, index)
		end
	end)

	props:addObserver('selectedJobValue', function(propTable, key, value)
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
