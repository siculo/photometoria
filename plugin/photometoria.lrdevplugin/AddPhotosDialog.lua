-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrColor = import 'LrColor'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrBinding = import 'LrBinding'
local LrPrefs = import 'LrPrefs'
local LrProgressScope = import 'LrProgressScope'
local LrTasks = import 'LrTasks'

local LrApplication = import 'LrApplication'

local CatalogIdentity = require 'CatalogIdentity'
local ServerConnection = require 'ServerConnection'
local PhotoValidator = require 'PhotoValidator'
local TaskDialogUI = require 'TaskDialogUI'
local PhotoUploader = require 'PhotoUploader'
local Guard = require 'Guard'
local TaskUtils = require 'TaskUtils'

local bind = LrView.bind
local LABEL_WIDTH = 80

--- Updates the existing task summary text.
local function updateExistingTaskSummary(props, tasks)
	if #props.existingTaskItems == 0 then
		props.existingTaskSummary = LOC "$$$/Photometoria/AddPhotos/NoTasks=No tasks on the server. Create a new task first."
		props.existingTaskAfterSummary = ''
		return
	end

	local taskIndex = props.existingTask
	local task = tasks[taskIndex]
	if not task then
		props.existingTaskSummary = ''
		props.existingTaskAfterSummary = ''
		return
	end

	local photoCount = task.photo_count or 0
	local afterCount = photoCount + props.uploadableCount

	props.existingTaskSummary = string.format(
		'%d %s',
		photoCount,
		LOC "$$$/Photometoria/AddPhotos/PhotosPresent=photos already present"
	)

	props.existingTaskAfterSummary = string.format(
		'%s %d %s',
		LOC "$$$/Photometoria/AddPhotos/AfterAdding=After adding:",
		afterCount,
		LOC "$$$/Photometoria/AddPhotos/Photos=photos"
	)
end

--- Updates the confirm button enabled state.
local function updateConfirmEnabled(props)
	if props.uploadableCount < 1 then
		props.confirmEnabled = false
		return
	end

	if props.destination == 'new' then
		local nameOk = (props.taskName ~= nil and props.taskName ~= '')
		local contextOk = (props.taskContext ~= nil and props.taskContext ~= '')
		local withinLimit = #(props.taskContext or '') <= props.maxContextLength
		props.confirmEnabled = nameOk and contextOk and withinLimit
	else
		props.confirmEnabled = (#props.existingTaskItems > 0)
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
local function initProperties(props, validation, serverData, tasks, prefs)
	props.uploadableCount = validation.uploadable
	props.photoSummary = PhotoValidator.formatSummary(validation)
	props.storageFree = LOC("$$$/Photometoria/AddPhotos/StorageFree=Available storage: ^1", serverData.storageFree)

	local hasTasks = (#tasks > 0)
	props.destination = hasTasks and 'existing' or 'new'
	props.newTaskVisible = not hasTasks
	props.existingTaskVisible = hasTasks

	props.maxContextLength = serverData.maxContextLength
	props.taskName = ''
	props.taskContext = ''
	props.contextCounter = '0/' .. serverData.maxContextLength
	props.contextCounterOk = false
	props.contextCounterError = true

	props.existingTaskItems = buildTaskPopupItems(tasks)
	props.existingTask = TaskUtils.findTaskIndex(tasks, prefs.lastActiveTaskId) or 1
	props.existingTaskSummary = ''
	props.existingTaskAfterSummary = ''

	props.confirmEnabled = false

	if hasTasks then
		updateExistingTaskSummary(props, tasks)
		updateConfirmEnabled(props)
	end
end

--- Builds the photo summary section.
local function buildPhotoSection(f, props)
	return f:group_box {
		title = LOC "$$$/Photometoria/AddPhotos/PhotoSectionTitle=Photos to add",
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:static_text {
			title = bind 'photoSummary',
			fill_horizontal = 1,
		},

		f:static_text {
			title = bind 'storageFree',
			fill_horizontal = 1,
			font = '<system/small>',
		},
	}
end

--- Builds the destination section (linear layout).
local function buildDestinationSection(f, props)
	return f:group_box {
		title = LOC "$$$/Photometoria/AddPhotos/DestSectionTitle=Destination",
		spacing = f:control_spacing(),
		fill_horizontal = 1,

		f:radio_button {
			title = LOC "$$$/Photometoria/AddPhotos/ExistingTask=Add to existing task",
			value = bind 'destination',
			checked_value = 'existing',
		},

		f:row {
			spacing = f:label_spacing(),

			f:spacer { width = LABEL_WIDTH },

			f:popup_menu {
				value = bind 'existingTask',
				items = bind 'existingTaskItems',
				enabled = bind 'existingTaskVisible',
				width = 250,
			},
		},

		f:row {
			spacing = f:label_spacing(),

			f:spacer { width = LABEL_WIDTH },

			f:static_text {
				title = bind 'existingTaskSummary',
				fill_horizontal = 1,
				font = '<system/small>',
			},
		},

		f:row {
			spacing = f:label_spacing(),

			f:spacer { width = LABEL_WIDTH },

			f:static_text {
				title = bind 'existingTaskAfterSummary',
				fill_horizontal = 1,
				font = '<system/small>',
			},
		},

		f:spacer { height = 8 },

		f:radio_button {
			title = LOC "$$$/Photometoria/AddPhotos/NewTask=Add to new task",
			value = bind 'destination',
			checked_value = 'new',
		},

		f:row {
			spacing = f:label_spacing(),

			f:static_text {
				title = LOC "$$$/Photometoria/AddPhotos/Name=Name",
				alignment = 'right',
				width = LABEL_WIDTH,
				enabled = bind 'newTaskVisible',
			},

			f:edit_field {
				value = bind 'taskName',
				enabled = bind 'newTaskVisible',
				fill_horizontal = 1,
				immediate = true,
			},
		},

		f:row {
			spacing = f:label_spacing(),

			f:static_text {
				title = LOC "$$$/Photometoria/AddPhotos/Context=Context",
				alignment = 'right',
				width = LABEL_WIDTH,
				enabled = bind 'newTaskVisible',
			},

			f:edit_field {
				value = bind 'taskContext',
				enabled = bind 'newTaskVisible',
				fill_horizontal = 1,
				height_in_lines = 6,
				immediate = true,
			},
		},

		f:row {
			spacing = f:label_spacing(),

			f:spacer { width = LABEL_WIDTH },

			f:static_text {
				title = LOC "$$$/Photometoria/AddPhotos/ContextHint=Context helps the model generate more precise and contextual tags.",
				fill_horizontal = 1,
				font = '<system/small>',
				enabled = bind 'newTaskVisible',
			},

			f:static_text {
				title = bind 'contextCounter',
				width_in_chars = 10,
				font = '<system/small>',
				alignment = 'right',
				enabled = bind 'newTaskVisible',
				visible = bind 'contextCounterOk',
			},

			f:static_text {
				title = bind 'contextCounter',
				width_in_chars = 10,
				font = '<system/small>',
				alignment = 'right',
				text_color = LrColor(0.85, 0, 0),
				enabled = bind 'newTaskVisible',
				visible = bind 'contextCounterError',
			},
		},

	}
end

--- Builds the complete dialog contents.
local function buildContents(f, props)
	return f:column {
		bind_to_object = props,
		spacing = f:control_spacing(),
		fill_horizontal = 1,
		width = 490,

		buildPhotoSection(f, props),

		buildDestinationSection(f, props),
	}
end

LrTasks.startAsyncTask(function()
	if not Guard.acquire('AddPhotosDialog') then
		LrDialogs.message(
			LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
			LOC "$$$/Photometoria/AddPhotos/AlreadyRunning=Add Photos is already in progress.",
			'info'
		)
		return
	end

	local prefs = LrPrefs.prefsForPlugin()
	local host = prefs.serverHost or ''

	if not ServerConnection.isValidHostPort(host) then
		LrDialogs.message(
			LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
			LOC "$$$/Photometoria/Error/NoServer=Server not configured. Please set the server address in Plugin Manager.",
			'critical'
		)
		Guard.release('AddPhotosDialog')
		return
	end

	local progressScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/Connecting=Connecting to server...",
	}

	local success, data = ServerConnection.info(host)
	progressScope:done()

	if not success then
		LrDialogs.message(
			LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
			data.message,
			'critical'
		)
		Guard.release('AddPhotosDialog')
		return
	end

	local catalog = LrApplication.activeCatalog()

	local validationScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/Validating=Checking photos...",
	}

	local validation = PhotoValidator.validate(catalog, validationScope)
	validationScope:done()

	if not validation then
		Guard.release('AddPhotosDialog')
		return
	end

	local catalogId = CatalogIdentity.catalogId()

	local taskScope = LrProgressScope {
		title = LOC "$$$/Photometoria/Progress/LoadingTasks=Loading tasks...",
	}

	local taskSuccess, tasks = ServerConnection.listTasks(host, catalogId)
	taskScope:done()

	if not taskSuccess then
		LrDialogs.message(
			LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
			tasks.message,
			'critical'
		)
		Guard.release('AddPhotosDialog')
		return
	end

	local confirmedTaskId = nil
	local isNewTask = false

	LrFunctionContext.callWithContext('AddPhotosDialog', function(context)
		local f = LrView.osFactory()

		local props = LrBinding.makePropertyTable(context)
		initProperties(props, validation, data, tasks, prefs)

		props:addObserver('destination', function(propTable, key, value)
			propTable.newTaskVisible = (value == 'new')
			propTable.existingTaskVisible = (value == 'existing')
			updateConfirmEnabled(propTable)
			if value == 'existing' then
				updateExistingTaskSummary(propTable, tasks)
			else
				propTable.existingTaskSummary = ''
				propTable.existingTaskAfterSummary = ''
			end
		end)

		props:addObserver('taskName', function(propTable)
			updateConfirmEnabled(propTable)
		end)

		props:addObserver('taskContext', function(propTable, _, value)
			local len = #(value or '')
			propTable.contextCounter = string.format('%d/%d', len, propTable.maxContextLength)
			propTable.contextCounterError = (len == 0) or (len > propTable.maxContextLength)
			propTable.contextCounterOk = not propTable.contextCounterError
			updateConfirmEnabled(propTable)
		end)

		props:addObserver('existingTask', function(propTable)
			updateExistingTaskSummary(propTable, tasks)
		end)

		local done = false
		while not done do
			local result = LrDialogs.presentModalDialog {
				title = LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
				contents = buildContents(f, props),
				actionVerb = LOC "$$$/Photometoria/Button/ConfirmAdd=Confirm",
				actionBinding = {
					enabled = {
						bind_to_object = props,
						key = 'confirmEnabled',
					},
				},
			}

			if result ~= 'ok' then
				done = true
			elseif props.destination == 'new' then
				local createScope = LrProgressScope {
					title = LOC "$$$/Photometoria/Progress/CreatingTask=Creating task...",
				}

				local ok, taskData = ServerConnection.createTask(host, catalogId, props.taskName, props.taskContext)
				createScope:done()

				if ok then
					confirmedTaskId = taskData.task_id
					prefs.lastActiveTaskId = taskData.task_id
					isNewTask = true
					done = true
				elseif taskData.duplicate then
					LrDialogs.message(
						LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
						LOC "$$$/Photometoria/Error/DuplicateName=A task with this name already exists. Please choose a different name.",
						'warning'
					)
				else
					LrDialogs.message(
						LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
						taskData.message,
						'critical'
					)
				end
			else
				local selectedTask = tasks[props.existingTask]
				if selectedTask then
					confirmedTaskId = selectedTask.task_id
					prefs.lastActiveTaskId = selectedTask.task_id
				end
				done = true
			end
		end
	end)

	Guard.release('AddPhotosDialog')

	local photosToUpload = validation.photos

	if confirmedTaskId and not isNewTask then
		local checkScope = LrProgressScope {
			title = LOC "$$$/Photometoria/Progress/CheckingExisting=Checking for existing photos...",
		}
		local listOk, existingData = ServerConnection.listTaskPhotos(host, confirmedTaskId)
		checkScope:done()

		if listOk then
			local existingIds = {}
			for _, p in ipairs(existingData.photos or {}) do
				if p.client_id then
					existingIds[p.client_id] = true
				end
			end

			local overlapping = {}
			local newOnly = {}
			for _, photo in ipairs(validation.photos) do
				local uid = photo:getRawMetadata('uuid')
				if existingIds[uid] then
					overlapping[#overlapping + 1] = photo
				else
					newOnly[#newOnly + 1] = photo
				end
			end

			if #overlapping > 0 then
				local replaceExisting = true
				local proceed = false

				LrFunctionContext.callWithContext('ConfirmReplaceDialog', function(ctx)
					local f = LrView.osFactory()
					local dlgProps = LrBinding.makePropertyTable(ctx)
					dlgProps.replaceExisting = true

					local result = LrDialogs.presentModalDialog {
						title = LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
						contents = f:column {
							bind_to_object = dlgProps,
							spacing = f:control_spacing(),
							fill_horizontal = 1,
							width = 420,

							f:static_text {
								title = LOC("$$$/Photometoria/Upload/ConfirmReplace=^1 of ^2 photos already exist in this task.",
									tostring(#overlapping), tostring(#validation.photos)),
								fill_horizontal = 1,
							},

							f:checkbox {
								title = LOC "$$$/Photometoria/Upload/ReplaceExisting=Update existing photos",
								value = bind 'replaceExisting',
							},
						},
						actionVerb = LOC "$$$/Photometoria/Button/Continue=Continue",
					}

					if result == 'ok' then
						proceed = true
						replaceExisting = dlgProps.replaceExisting
					end
				end)

				if not proceed then
					confirmedTaskId = nil
				elseif not replaceExisting then
					if #newOnly == 0 then
						LrDialogs.message(
							LOC "$$$/Photometoria/AddPhotos/Title=Add Photos",
							LOC "$$$/Photometoria/Upload/NothingToUpload=All selected photos already exist in the task. No photos to upload.",
							'info'
						)
						confirmedTaskId = nil
					else
						photosToUpload = newOnly
					end
				end
			end
		end
	end

	if confirmedTaskId then
		local uploadResult = PhotoUploader.run(host, confirmedTaskId, photosToUpload, data.maxPhotosPerRequest, data.maxPhotoSizeBytes)

		local title = LOC "$$$/Photometoria/AddPhotos/Title=Add Photos"
		local created = uploadResult.created or 0
		local replaced = uploadResult.replaced or 0
		local totalSuccessful = created + replaced
		local oversized = uploadResult.oversized or 0

		if uploadResult.storage_full then
			LrDialogs.message(
				title,
				LOC("$$$/Photometoria/Upload/StorageFull=Upload aborted: server storage is full. ^1 of ^2 photos were uploaded before the limit was reached.",
					tostring(totalSuccessful), tostring(uploadResult.total)),
				'critical'
			)
		elseif uploadResult.cancelled then
			LrDialogs.message(
				title,
				LOC("$$$/Photometoria/Upload/Cancelled=Upload cancelled. ^1 of ^2 photos uploaded.",
					tostring(totalSuccessful), tostring(uploadResult.total)),
				'info'
			)
		else
			local body
			if uploadResult.failed == 0 and oversized == 0 then
				if replaced == 0 then
					body = LOC("$$$/Photometoria/Upload/Success=^1 photos uploaded successfully. Open the task window?",
						tostring(created))
				elseif created == 0 then
					body = LOC("$$$/Photometoria/Upload/SuccessReplaced=^1 photos updated successfully. Open the task window?",
						tostring(replaced))
				else
					body = LOC("$$$/Photometoria/Upload/SuccessMixed=^1 new photos added, ^2 updated. Open the task window?",
						tostring(created), tostring(replaced))
				end
			elseif uploadResult.failed == 0 then
				if replaced == 0 then
					body = LOC("$$$/Photometoria/Upload/SuccessWithOversized=^1 photos uploaded (^2 skipped: file too large for server limit). Open the task window?",
						tostring(created), tostring(oversized))
				elseif created == 0 then
					body = LOC("$$$/Photometoria/Upload/SuccessReplacedWithOversized=^1 photos updated (^2 skipped: file too large for server limit). Open the task window?",
						tostring(replaced), tostring(oversized))
				else
					body = LOC("$$$/Photometoria/Upload/SuccessMixedWithOversized=^1 new, ^2 updated (^3 skipped: file too large for server limit). Open the task window?",
						tostring(created), tostring(replaced), tostring(oversized))
				end
			elseif oversized == 0 then
				body = LOC("$$$/Photometoria/Upload/Partial=^1 of ^2 photos uploaded (^3 failed). Open the task window?",
					tostring(totalSuccessful), tostring(uploadResult.total), tostring(uploadResult.failed))
			else
				body = LOC("$$$/Photometoria/Upload/PartialWithOversized=^1 of ^2 photos uploaded (^3 failed, ^4 too large for server limit). Open the task window?",
					tostring(totalSuccessful), tostring(uploadResult.total), tostring(uploadResult.failed), tostring(oversized))
			end

			local action = LrDialogs.confirm(
				title,
				body,
				LOC "$$$/Photometoria/Button/OpenTask=Open Task Window",
				LOC "$$$/Photometoria/Button/Close=Close"
			)

			if action == 'ok' then
				local refreshOk, refreshedTasks = ServerConnection.listTasks(host, catalogId)
				if refreshOk then
					TaskDialogUI.showDialog(host, refreshedTasks, data.maxContextLength)
				end
			end
		end
	end
end)
