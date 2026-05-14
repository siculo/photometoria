-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrDialogs = import 'LrDialogs'
local LrPrefs = import 'LrPrefs'
local LrProgressScope = import 'LrProgressScope'
local LrTasks = import 'LrTasks'

local CatalogIdentity = require 'CatalogIdentity'
local ServerConnection = require 'ServerConnection'
local TaskDialogUI = require 'TaskDialogUI'
local RecentHosts = require 'RecentHosts'

LrTasks.startAsyncTask(function()
	local prefs = LrPrefs.prefsForPlugin()
	local host = RecentHosts.extractHost(prefs.serverHost or '')

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

	local success, data = ServerConnection.info(host)
	progressScope:done()

	if not success then
		LrDialogs.message(
			LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
			data.message,
			'critical'
		)
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
			LOC "$$$/Photometoria/Dialog/Title=Photometoria Tasks",
			tasks.message,
			'critical'
		)
		return
	end

	TaskDialogUI.showDialog(host, tasks, data.maxContextLength)
end)
