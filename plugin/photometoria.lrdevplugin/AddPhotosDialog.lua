-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrBinding = import 'LrBinding'
local LrPrefs = import 'LrPrefs'
local LrProgressScope = import 'LrProgressScope'
local LrTasks = import 'LrTasks'

local ServerConnection = require 'ServerConnection'
local MockData = require 'MockData'

local bind = LrView.bind
local LABEL_WIDTH = 80

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

--- Returns the mock photo data for the current photo choice.
local function getPhotoData(choice)
	if choice == 'all' then
		return MockData.allPhotos
	end
	return MockData.selectedPhotos
end

--- Updates the photo summary text.
local function updatePhotoSummary(props)
	local data = getPhotoData(props.photoChoice)
	props.photoSummary = string.format(
		'%d %s \194\183 %s %s',
		data.count,
		LOC "$$$/Photometoria/AddPhotos/PhotosLabel=photos selected",
		formatBytes(data.sizeBytes),
		LOC "$$$/Photometoria/AddPhotos/Estimated=estimated"
	)
end

--- Updates the existing task summary text.
local function updateExistingTaskSummary(props)
	local taskIndex = props.existingTask
	local task = MockData.tasks[taskIndex]
	if not task then
		props.existingTaskSummary = ''
		return
	end

	local photoData = getPhotoData(props.photoChoice)
	local afterCount = task.photoCount + photoData.count
	local afterSize = task.sizeBytes + photoData.sizeBytes

	props.existingTaskSummary = string.format(
		'%d %s \194\183 %s\n%s ~%d %s \194\183 ~%s',
		task.photoCount,
		LOC "$$$/Photometoria/AddPhotos/PhotosPresent=photos already present",
		formatBytes(task.sizeBytes),
		LOC "$$$/Photometoria/AddPhotos/AfterAdding=After adding:",
		afterCount,
		LOC "$$$/Photometoria/AddPhotos/Photos=photos",
		formatBytes(afterSize)
	)
end

--- Updates the confirm button enabled state.
local function updateConfirmEnabled(props)
	if props.destination == 'new' then
		props.confirmEnabled = (props.taskName ~= nil and props.taskName ~= '')
	else
		props.confirmEnabled = true
	end
end

--- Builds popup_menu items from the task list.
local function buildTaskPopupItems(tasks)
	local items = {}
	for i, task in ipairs(tasks) do
		items[#items + 1] = {
			title = task.name,
			value = i,
		}
	end
	return items
end

--- Initializes all bindable properties.
local function initProperties(props)
	props.photoChoice = 'selected'
	props.photoSummary = ''

	props.destination = 'new'
	props.newTaskVisible = true
	props.existingTaskVisible = false

	props.taskName = ''
	props.taskContext = ''

	props.existingTask = 1
	props.existingTaskItems = buildTaskPopupItems(MockData.tasks)
	props.existingTaskSummary = ''

	props.confirmEnabled = false

	updatePhotoSummary(props)
end

--- Builds the photo selection section.
local function buildPhotoSection(f, props)
	return f:column {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:radio_button {
			title = LOC "$$$/Photometoria/AddPhotos/SelectedOnly=Selected only",
			value = bind 'photoChoice',
			checked_value = 'selected',
		},

		f:radio_button {
			title = LOC "$$$/Photometoria/AddPhotos/All=All photos in catalog",
			value = bind 'photoChoice',
			checked_value = 'all',
		},

		f:static_text {
			title = bind 'photoSummary',
			fill_horizontal = 1,
		},
	}
end

--- Builds the destination selection section.
local function buildDestinationSection(f, props)
	return f:column {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:static_text {
			title = LOC "$$$/Photometoria/AddPhotos/DestTitle=Destination",
			font = '<system/bold>',
		},

		f:radio_button {
			title = LOC "$$$/Photometoria/AddPhotos/NewTask=New task",
			value = bind 'destination',
			checked_value = 'new',
		},

		f:radio_button {
			title = LOC "$$$/Photometoria/AddPhotos/ExistingTask=Existing task",
			value = bind 'destination',
			checked_value = 'existing',
		},
	}
end

--- Builds the new task form.
local function buildNewTaskForm(f, props)
	return f:column {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:static_text {
			visible = bind 'newTaskVisible',
			title = LOC "$$$/Photometoria/AddPhotos/NewTaskTitle=New Task",
			font = '<system/bold>',
		},

		f:row {
			visible = bind 'newTaskVisible',
			spacing = f:label_spacing(),

			f:static_text {
				title = LOC "$$$/Photometoria/AddPhotos/Name=Name",
				alignment = 'right',
				width = LABEL_WIDTH,
			},

			f:edit_field {
				value = bind 'taskName',
				fill_horizontal = 1,
				immediate = true,
			},
		},

		f:row {
			visible = bind 'newTaskVisible',
			spacing = f:label_spacing(),

			f:static_text {
				title = LOC "$$$/Photometoria/AddPhotos/Context=Context",
				alignment = 'right',
				width = LABEL_WIDTH,
			},

			f:edit_field {
				value = bind 'taskContext',
				fill_horizontal = 1,
				height_in_lines = 4,
			},
		},

		f:static_text {
			visible = bind 'newTaskVisible',
			title = LOC "$$$/Photometoria/AddPhotos/ContextHint=Context helps the model generate more precise and contextual tags.",
			fill_horizontal = 1,
		},
	}
end

--- Builds the existing task picker.
local function buildExistingTaskForm(f, props)
	return f:column {
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:popup_menu {
			visible = bind 'existingTaskVisible',
			value = bind 'existingTask',
			items = bind 'existingTaskItems',
			width = 250,
		},

		f:static_text {
			visible = bind 'existingTaskVisible',
			title = bind 'existingTaskSummary',
			fill_horizontal = 1,
			height_in_lines = 2,
		},
	}
end

--- Builds the complete dialog contents.
local function buildContents(f, props)
	return f:row {
		bind_to_object = props,
		spacing = f:dialog_spacing(),
		fill_horizontal = 1,

		f:column {
			width = 200,
			spacing = f:control_spacing(),

			buildPhotoSection(f, props),

			f:separator { fill_horizontal = 1 },

			buildDestinationSection(f, props),
		},

		f:column {
			fill_horizontal = 1,
			spacing = f:control_spacing(),

			buildNewTaskForm(f, props),
			buildExistingTaskForm(f, props),
		},
	}
end

LrTasks.startAsyncTask(function()
	local prefs = LrPrefs.prefsForPlugin()
	local host = prefs.serverHost or ''

	if not ServerConnection.isValidHostPort(host) then
		LrDialogs.message(
			LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
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
			LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
			data.message,
			'critical'
		)
		return
	end

	LrFunctionContext.callWithContext('AddPhotosDialog', function(context)
		local f = LrView.osFactory()

		local props = LrBinding.makePropertyTable(context)
		initProperties(props)

		props:addObserver('photoChoice', function(propTable)
			updatePhotoSummary(propTable)
			updateExistingTaskSummary(propTable)
			updateConfirmEnabled(propTable)
		end)

		props:addObserver('destination', function(propTable, key, value)
			propTable.newTaskVisible = (value == 'new')
			propTable.existingTaskVisible = (value == 'existing')
			updateConfirmEnabled(propTable)
			if value == 'existing' then
				updateExistingTaskSummary(propTable)
			else
				propTable.existingTaskSummary = ''
			end
		end)

		props:addObserver('taskName', function(propTable)
			updateConfirmEnabled(propTable)
		end)

		props:addObserver('existingTask', function(propTable)
			updateExistingTaskSummary(propTable)
		end)

		local result = LrDialogs.presentModalDialog {
			title = LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
			contents = buildContents(f, props),
			actionVerb = LOC "$$$/Photometoria/Button/ConfirmAdd=Confirm and Go to Task",
			actionBinding = {
				enabled = {
					bind_to_object = props,
					key = 'confirmEnabled',
				},
			},
		}

		if result == 'ok' then
			LrDialogs.message(
				LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
				LOC "$$$/Photometoria/Mock/AddPhotosMsg=This would add the photos and open the Task window.",
				'info'
			)
		end
	end)
end)
