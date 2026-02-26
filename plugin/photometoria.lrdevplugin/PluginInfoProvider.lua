-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrView = import 'LrView'
local LrColor = import 'LrColor'

local ServerConnection = require 'ServerConnection'

local bind = LrView.bind
local LABEL_WIDTH = 110

local function hideStatus(propTable)
	propTable.statusLabel = ''
	propTable.statusText = ''
	propTable.statusMessage = ''
end

local function hideDetails(propTable)
	propTable.detailStorageAllocLabel = ''
	propTable.detailStorageAllocValue = ''
	propTable.detailStorageUsedLabel = ''
	propTable.detailStorageUsedValue = ''
	propTable.detailStorageFreeLabel = ''
	propTable.detailStorageFreeValue = ''
	propTable.detailProvidersLabel = ''
	propTable.detailProvidersValue = ''
	propTable.detailDefaultLabel = ''
	propTable.detailDefaultValue = ''
	propTable.detailVersionLabel = ''
	propTable.detailVersionValue = ''
	propTable.detailTasksLabel = ''
	propTable.detailTasksValue = ''
	propTable.detailJobsLabel = ''
	propTable.detailJobsValue = ''
end

local function showDetails(propTable, data)
	propTable.detailStorageAllocLabel = 'Storage allocated:'
	propTable.detailStorageAllocValue = data.storageAllocated
	propTable.detailStorageUsedLabel = 'Storage used:'
	propTable.detailStorageUsedValue = data.storageUsed
	propTable.detailStorageFreeLabel = 'Storage free:'
	propTable.detailStorageFreeValue = data.storageFree
	propTable.detailProvidersLabel = 'Providers:'
	propTable.detailProvidersValue = data.providers
	propTable.detailDefaultLabel = 'Default provider:'
	propTable.detailDefaultValue = data.defaultProvider
	propTable.detailVersionLabel = 'Server version:'
	propTable.detailVersionValue = data.version
	propTable.detailTasksLabel = 'Active tasks:'
	propTable.detailTasksValue = tostring(data.activeTasks)
	propTable.detailJobsLabel = 'Queued jobs:'
	propTable.detailJobsValue = tostring(data.queuedJobs)
end

local function sectionsForTopOfDialog(f, propertyTable)
	propertyTable.serverHost = ''
	propertyTable.connectEnabled = false
	propertyTable.validationMessage = ''
	propertyTable.connecting = false
	propertyTable.synopsis = ''

	propertyTable.statusLabel = ''
	propertyTable.statusText = ''
	propertyTable.statusMessage = ''

	hideDetails(propertyTable)

	propertyTable:addObserver('serverHost', function(propTable, key, value)
		hideStatus(propTable)
		hideDetails(propTable)
		propTable.synopsis = ''

		if value == nil or value == '' then
			propTable.connectEnabled = false
			propTable.validationMessage = ''
			return
		end

		if ServerConnection.isValidHostPort(value) then
			propTable.connectEnabled = not propTable.connecting
			propTable.validationMessage = ''
		else
			propTable.connectEnabled = false
			propTable.validationMessage = 'Invalid format. Use host:port (e.g. 192.168.1.50:8080)'
		end
	end)

	local function onConnect()
		local host = propertyTable.serverHost
		if not ServerConnection.isValidHostPort(host) then
			return
		end

		propertyTable.connecting = true
		propertyTable.connectEnabled = false
		hideDetails(propertyTable)

		propertyTable.statusLabel = 'Status:'
		propertyTable.statusText = 'Connecting...'
		propertyTable.statusMessage = ''
		propertyTable.synopsis = 'Connecting...'

		ServerConnection.simulateConnect(host, function(success, data)
			propertyTable.connecting = false

			if success then
				propertyTable.statusText = 'Online'
				propertyTable.statusMessage = 'Connected to ' .. host
				propertyTable.synopsis = 'Online'
				showDetails(propertyTable, data)
			else
				propertyTable.statusText = 'Unreachable'
				propertyTable.statusMessage = data.message
				propertyTable.synopsis = 'Unreachable'
			end

			if ServerConnection.isValidHostPort(propertyTable.serverHost) then
				propertyTable.connectEnabled = true
			end
		end)
	end

	return {
		{
			title = 'Photometoria — Setup Server',
			synopsis = bind { key = 'synopsis', object = propertyTable },

			f:column {
				bind_to_object = propertyTable,
				fill_horizontal = 1,

				f:row {
					spacing = f:control_spacing(),

					f:static_text {
						title = 'Server:',
						alignment = 'right',
						width = LABEL_WIDTH,
					},

					f:edit_field {
						value = bind 'serverHost',
						width_in_chars = 25,
						immediate = true,
					},

					f:push_button {
						title = 'Connect',
						enabled = bind 'connectEnabled',
						action = onConnect,
					},
				},

				f:row {
					spacing = f:control_spacing(),

					f:spacer { width = LABEL_WIDTH },

					f:static_text {
						title = bind 'validationMessage',
						text_color = LrColor(0.80, 0.15, 0.15),
						fill_horizontal = 1,
					},
				},

				f:spacer { height = 4 },

				f:row {
					spacing = f:control_spacing(),

					f:static_text {
						title = bind 'statusLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},

					f:static_text {
						title = bind 'statusText',
						font = '<system/bold>',
						fill_horizontal = 1,
					},
				},

				f:row {
					spacing = f:control_spacing(),

					f:spacer { width = LABEL_WIDTH },

					f:static_text {
						title = bind 'statusMessage',
						fill_horizontal = 1,
					},
				},

				f:spacer { height = 10 },
				f:separator { fill_horizontal = 1 },
				f:spacer { height = 8 },

				f:row {
					f:static_text {
						title = bind 'detailStorageAllocLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailStorageAllocValue',
						fill_horizontal = 1,
					},
				},

				f:row {
					f:static_text {
						title = bind 'detailStorageUsedLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailStorageUsedValue',
						fill_horizontal = 1,
					},
				},

				f:row {
					f:static_text {
						title = bind 'detailStorageFreeLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailStorageFreeValue',
						fill_horizontal = 1,
					},
				},

				f:row {
					f:static_text {
						title = bind 'detailProvidersLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailProvidersValue',
						fill_horizontal = 1,
					},
				},

				f:row {
					f:static_text {
						title = bind 'detailDefaultLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailDefaultValue',
						fill_horizontal = 1,
					},
				},

				f:row {
					f:static_text {
						title = bind 'detailVersionLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailVersionValue',
						fill_horizontal = 1,
					},
				},

				f:row {
					f:static_text {
						title = bind 'detailTasksLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailTasksValue',
						fill_horizontal = 1,
					},
				},

				f:row {
					f:static_text {
						title = bind 'detailJobsLabel',
						alignment = 'right',
						width = LABEL_WIDTH,
					},
					f:static_text {
						title = bind 'detailJobsValue',
						fill_horizontal = 1,
					},
				},
			},
		},
	}
end

return {
	sectionsForTopOfDialog = sectionsForTopOfDialog,
}
