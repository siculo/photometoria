-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

--- Manages a persistent UUID identity for the active Lightroom catalog.
--- The identifier is stored in the catalog's plugin preferences and
--- lazy-initialized on first access.

local LrApplication = import 'LrApplication'
local LrDate = import 'LrDate'

local UUID = require 'UUID'

local CatalogIdentity = {}

local PROPERTY_KEY = 'catalogId'

--- Seeds the PRNG with high-resolution time from the Lightroom SDK.
local function seedRng()
	local time = LrDate.currentTime()
	local fractional = time - math.floor(time)
	local seed = math.floor(time) + math.floor(fractional * 1000000)
	math.randomseed(seed)
	math.random(); math.random(); math.random()
end

--- Returns the catalog's persistent UUID, creating one if it doesn't exist.
--- Must be called from within an async task context.
function CatalogIdentity.catalogId()
	local catalog = LrApplication.activeCatalog()
	local id = catalog:getPropertyForPlugin(_PLUGIN, PROPERTY_KEY)

	if not id then
		seedRng()
		id = UUID.generate()
		catalog:withPrivateWriteAccessDo(function()
			catalog:setPropertyForPlugin(_PLUGIN, PROPERTY_KEY, id)
		end)
	end

	return id
end

return CatalogIdentity
