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
	German   = LOC "$$$/Photometoria/Language/German=German",
	Spanish  = LOC "$$$/Photometoria/Language/Spanish=Spanish",
	Arabic   = LOC "$$$/Photometoria/Language/Arabic=Arabic",
	Chinese    = LOC "$$$/Photometoria/Language/Chinese=Chinese",
	Portuguese = LOC "$$$/Photometoria/Language/Portuguese=Portuguese",
	Russian    = LOC "$$$/Photometoria/Language/Russian=Russian",
	Japanese   = LOC "$$$/Photometoria/Language/Japanese=Japanese",
	Hindi      = LOC "$$$/Photometoria/Language/Hindi=Hindi",
	Korean     = LOC "$$$/Photometoria/Language/Korean=Korean",
	Turkish    = LOC "$$$/Photometoria/Language/Turkish=Turkish",
	Dutch      = LOC "$$$/Photometoria/Language/Dutch=Dutch",
	Polish     = LOC "$$$/Photometoria/Language/Polish=Polish",
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

--- Returns the saved model name if still available in modelItems, else nil.
local function resolveSavedModel(modelItems, savedModelName)
	if not savedModelName then return nil end
	for _, item in ipairs(modelItems) do
		if item.value == savedModelName then return savedModelName end
	end
	return nil
end

--- Shows the New Job dialog. Returns {provider, model, language, photo_ids} or nil if cancelled.
--- providers: array of {name, default_language, models: [{name, description, available, supported_languages}]}
--- photoCount: number of photos in the task
--- defaultProviderName: optional provider name to pre-select (falls back to saved prefs)
--- selectionPhotoIds: optional array of server-side photo_id strings for the current Lightroom
---   selection that overlaps the task; nil disables the "selection" scope option.
function NewJobDialog.showDialog(providers, photoCount, defaultProviderName, selectionPhotoIds)
	local result = nil
	local prefs = LrPrefs.prefsForPlugin()
	local savedLanguage = prefs.lastLanguage
	local savedProviderName = prefs.lastProviderName
	local savedModelName = prefs.lastModelName

	local defaultIndex = 1
	local preferredProviderName = savedProviderName or defaultProviderName
	if preferredProviderName then
		for i, p in ipairs(providers) do
			if p.name == preferredProviderName then
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
		local initialModel = resolveSavedModel(modelItems, savedModelName)
			or firstAvailableModel(firstProvider)
		local firstModelObj = findModel(firstProvider, initialModel)
		local languageItems = buildLanguageItems(firstModelObj)

		props.providerItems = buildProviderItems(providers)
		props.selectedProvider = defaultIndex
		props.modelItems = modelItems
		props.selectedModel = initialModel
		props.noModelsVisible = (#modelItems == 0)
		props.languageItems = languageItems
		props.selectedLanguage = selectLanguage(languageItems, savedLanguage, firstProvider.default_language)
		props.languageVisible = (#languageItems > 0)
		props.confirmEnabled = (initialModel ~= nil)

		props.jobScope = 'all'
		props.scopeAllLabel = LOC("$$$/Photometoria/NewJob/ScopeAll=All photos in task (^1)", tostring(photoCount))
		if selectionPhotoIds == nil then
			props.scopeSelectionLabel = LOC "$$$/Photometoria/NewJob/ScopeSelectionEmpty=Selected in Lightroom (no photos selected)"
			props.scopeSelectionEnabled = false
		else
			local selectionCount = #selectionPhotoIds
			if selectionCount > 0 and selectionCount < photoCount then
				props.scopeSelectionLabel = LOC("$$$/Photometoria/NewJob/ScopeSelection=Selected in Lightroom (^1)", tostring(selectionCount))
				props.scopeSelectionEnabled = true
			elseif selectionCount == photoCount then
				props.scopeSelectionLabel = LOC "$$$/Photometoria/NewJob/ScopeSelectionAll=Selected in Lightroom (all task photos)"
				props.scopeSelectionEnabled = false
			else
				props.scopeSelectionLabel = LOC "$$$/Photometoria/NewJob/ScopeSelectionNone=Selected in Lightroom (no overlap)"
				props.scopeSelectionEnabled = false
			end
		end

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

			f:group_box {
				title = LOC "$$$/Photometoria/NewJob/ScopeSection=Scope",
				fill_horizontal = 1,
				spacing = f:control_spacing(),

				f:radio_button {
					title = bind 'scopeAllLabel',
					value = bind 'jobScope',
					checked_value = 'all',
				},

				f:radio_button {
					title = bind 'scopeSelectionLabel',
					value = bind 'jobScope',
					checked_value = 'selection',
					enabled = bind 'scopeSelectionEnabled',
				},
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
			prefs.lastProviderName = provider.name
			prefs.lastModelName = props.selectedModel
			result = {
				provider = provider.name,
				model = props.selectedModel,
				language = language,
				photo_ids = (props.jobScope == 'selection') and selectionPhotoIds or nil,
			}
		end
	end)

	return result
end

return NewJobDialog
