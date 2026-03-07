-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrBinding = import 'LrBinding'

local bind = LrView.bind

local NewJobDialog = {}

--- Builds popup_menu items from a provider's model list.
local function buildModelItems(provider)
	local items = {}
	for _, model in ipairs(provider.models) do
		items[#items + 1] = { title = model, value = model }
	end
	return items
end

--- Builds popup_menu items from the providers list.
local function buildProviderItems(providers)
	local items = {}
	for i, provider in ipairs(providers) do
		items[#items + 1] = { title = provider.name, value = i }
	end
	return items
end

--- Formats the cost info text for a provider, or empty string if local.
local function formatCostInfo(provider, photoCount)
	if not provider.estimatedCost then
		return ''
	end
	return LOC(
		"$$$/Photometoria/NewJob/CostEstimate=Estimated cost: ~^1 for ^2 photos",
		provider.estimatedCost,
		photoCount
	)
end

--- Shows the New Job dialog. Returns {provider, model} or nil if cancelled.
function NewJobDialog.showDialog(providers, photoCount)
	local result = nil

	LrFunctionContext.callWithContext('NewJobDialog', function(context)
		local f = LrView.osFactory()
		local props = LrBinding.makePropertyTable(context)

		props.providerItems = buildProviderItems(providers)
		props.selectedProvider = 1
		props.modelItems = buildModelItems(providers[1])
		props.selectedModel = providers[1].models[1]
		props.costInfoText = formatCostInfo(providers[1], photoCount)
		props.costInfoVisible = (providers[1].estimatedCost ~= nil)
		props.photoSummary = string.format(
			'%d %s',
			photoCount,
			LOC "$$$/Photometoria/NewJob/PhotosToProcess=photos to process"
		)

		props:addObserver('selectedProvider', function(propTable, key, value)
			local provider = providers[value]
			if not provider then
				return
			end
			propTable.modelItems = buildModelItems(provider)
			propTable.selectedModel = provider.models[1]
			propTable.costInfoText = formatCostInfo(provider, photoCount)
			propTable.costInfoVisible = (provider.estimatedCost ~= nil)
		end)

		local contents = f:column {
			bind_to_object = props,
			spacing = f:control_spacing(),
			fill_horizontal = 1,

			f:group_box {
				title = LOC "$$$/Photometoria/NewJob/ModelSection=Model",
				fill_horizontal = 1,
				spacing = f:control_spacing(),

				f:row {
					spacing = f:label_spacing(),

					f:static_text {
						title = LOC "$$$/Photometoria/NewJob/Provider=Provider",
						width = 70,
					},

					f:popup_menu {
						value = bind 'selectedProvider',
						items = bind 'providerItems',
						width = 220,
					},
				},

				f:row {
					spacing = f:label_spacing(),

					f:static_text {
						title = LOC "$$$/Photometoria/NewJob/Model=Model",
						width = 70,
					},

					f:popup_menu {
						value = bind 'selectedModel',
						items = bind 'modelItems',
						width = 220,
					},
				},

				f:static_text {
					title = bind 'costInfoText',
					visible = bind 'costInfoVisible',
					fill_horizontal = 1,
					font = '<system/small>',
				},
			},

			f:static_text {
				title = bind 'photoSummary',
				fill_horizontal = 1,
			},
		}

		local dialogResult = LrDialogs.presentModalDialog {
			title = LOC "$$$/Photometoria/NewJob/Title=New Job",
			contents = contents,
			actionVerb = '\226\150\182 ' .. LOC "$$$/Photometoria/NewJob/Start=Start Job",
		}

		if dialogResult == 'ok' then
			local provider = providers[props.selectedProvider]
			if provider.estimatedCost then
				local confirmed = LrDialogs.confirm(
					LOC "$$$/Photometoria/NewJob/CostConfirmTitle=Cost confirmation",
					LOC(
						"$$$/Photometoria/NewJob/CostConfirmMsg=The provider ^1 has an estimated cost of ~^2 for ^3 photos. Continue?",
						provider.name,
						provider.estimatedCost,
						photoCount
					)
				)
				if confirmed ~= 'ok' then
					return
				end
			end
			result = {
				provider = provider.name,
				model = props.selectedModel,
			}
		end
	end)

	return result
end

return NewJobDialog
