-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrFunctionContext = import 'LrFunctionContext'
local LrDialogs = import 'LrDialogs'
local LrView = import 'LrView'
local LrBinding = import 'LrBinding'
local LrColor = import 'LrColor'
local LrPrefs = import 'LrPrefs'

local bind = LrView.bind

local NewJobDialog = {}

local LANGUAGE_LABELS = {
	English = LOC "$$$/Photometoria/Language/English=English",
	Italian = LOC "$$$/Photometoria/Language/Italian=Italian",
	French  = LOC "$$$/Photometoria/Language/French=French",
	German  = LOC "$$$/Photometoria/Language/German=German",
}

--- Returns the first available model name for a provider, or nil.
local function firstAvailableModel(provider)
	for _, model in ipairs(provider.models) do
		if model.available then
			return model.name
		end
	end
	return nil
end

--- Builds popup_menu items from a provider's available models.
local function buildModelItems(provider)
	local items = {}
	for _, model in ipairs(provider.models) do
		if model.available then
			local title = model.name
			if model.description then
				title = title .. ' \226\128\148 ' .. model.description
			end
			items[#items + 1] = { title = title, value = model.name }
		end
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

--- Returns the model object matching modelName within provider, or nil.
local function findModel(provider, modelName)
	if not provider or not modelName then return nil end
	for _, model in ipairs(provider.models) do
		if model.name == modelName then return model end
	end
	return nil
end

--- Builds language popup_menu items from a model's supported_languages list.
local function buildLanguageItems(model)
	if not model or not model.supported_languages then return {} end
	local items = {}
	for _, lang in ipairs(model.supported_languages) do
		items[#items + 1] = { title = LANGUAGE_LABELS[lang] or lang, value = lang }
	end
	return items
end

--- Selects a language: saved preference > provider default > first available.
local function selectLanguage(languageItems, savedLanguage, defaultLanguage)
	if #languageItems == 0 then return nil end
	for _, item in ipairs(languageItems) do
		if item.value == savedLanguage then return savedLanguage end
	end
	for _, item in ipairs(languageItems) do
		if item.value == defaultLanguage then return defaultLanguage end
	end
	return languageItems[1].value
end

--- Shows the New Job dialog. Returns {provider, model, language} or nil if cancelled.
--- providers: array of {name, default_language, models: [{name, description, available, supported_languages}]}
--- photoCount: number of photos in the task
--- defaultProviderName: optional provider name to pre-select
function NewJobDialog.showDialog(providers, photoCount, defaultProviderName)
	local result = nil
	local prefs = LrPrefs.prefsForPlugin()
	local savedLanguage = prefs.lastLanguage

	local defaultIndex = 1
	if defaultProviderName then
		for i, p in ipairs(providers) do
			if p.name == defaultProviderName then
				defaultIndex = i
				break
			end
		end
	end

	LrFunctionContext.callWithContext('NewJobDialog', function(context)
		local f = LrView.osFactory()
		local props = LrBinding.makePropertyTable(context)

		local firstProvider = providers[defaultIndex]
		local modelItems = buildModelItems(firstProvider)
		local firstModel = firstAvailableModel(firstProvider)
		local firstModelObj = findModel(firstProvider, firstModel)
		local languageItems = buildLanguageItems(firstModelObj)

		props.providerItems = buildProviderItems(providers)
		props.selectedProvider = defaultIndex
		props.modelItems = modelItems
		props.selectedModel = firstModel
		props.noModelsVisible = (#modelItems == 0)
		props.languageItems = languageItems
		props.selectedLanguage = selectLanguage(languageItems, savedLanguage, firstProvider.default_language)
		props.languageVisible = (#languageItems > 0)
		props.confirmEnabled = (firstModel ~= nil)
		props.photoSummary = string.format(
			'%d %s',
			photoCount,
			LOC "$$$/Photometoria/NewJob/PhotosToProcess=photos to process"
		)

		props:addObserver('selectedProvider', function(propTable, key, value)
			local provider = providers[value]
			if not provider then return end
			local items = buildModelItems(provider)
			local model = firstAvailableModel(provider)
			local modelObj = findModel(provider, model)
			local langItems = buildLanguageItems(modelObj)
			propTable.modelItems = items
			propTable.selectedModel = model
			propTable.noModelsVisible = (#items == 0)
			propTable.languageItems = langItems
			propTable.selectedLanguage = selectLanguage(langItems, propTable.selectedLanguage, provider.default_language)
			propTable.languageVisible = (#langItems > 0)
			propTable.confirmEnabled = (model ~= nil)
		end)

		props:addObserver('selectedModel', function(propTable, key, value)
			local provider = providers[propTable.selectedProvider]
			local modelObj = findModel(provider, value)
			local langItems = buildLanguageItems(modelObj)
			propTable.languageItems = langItems
			propTable.selectedLanguage = selectLanguage(langItems, propTable.selectedLanguage, provider and provider.default_language)
			propTable.languageVisible = (#langItems > 0)
			propTable.confirmEnabled = (value ~= nil)
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

				f:row {
					spacing = f:label_spacing(),

					f:static_text {
						title = LOC "$$$/Photometoria/NewJob/Language=Language",
						width = 70,
						visible = bind 'languageVisible',
					},

					f:popup_menu {
						value = bind 'selectedLanguage',
						items = bind 'languageItems',
						width = 220,
						visible = bind 'languageVisible',
					},
				},

				f:static_text {
					title = LOC "$$$/Photometoria/NewJob/NoModels=No available models for this provider",
					visible = bind 'noModelsVisible',
					fill_horizontal = 1,
					font = '<system/small>',
					text_color = LrColor(0.85, 0.2, 0.2),
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
			actionBinding = {
				enabled = {
					bind_to_object = props,
					key = 'confirmEnabled',
				},
			},
		}

		if dialogResult == 'ok' then
			local provider = providers[props.selectedProvider]
			local language = props.languageVisible and props.selectedLanguage or nil
			if language then
				prefs.lastLanguage = language
			end
			result = {
				provider = provider.name,
				model = props.selectedModel,
				language = language,
			}
		end
	end)

	return result
end

return NewJobDialog
