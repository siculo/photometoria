-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

return {
	LrSdkVersion = 10.0,
	LrSdkMinimumVersion = 4.0,

	LrPluginName = LOC "$$$/PluginInfo/Name=Photometoria Plugin",
	LrToolkitIdentifier = 'photometoria.plugin.lr',
	-- sdkDeprecation.action=log,

	LrPluginInfoProvider = 'PluginInfoProvider.lua',

	LrLibraryMenuItems = {
		{
			title = LOC "$$$/Photometoria/Menu/AddPhotos=Add Photos",
			file = 'AddPhotosDialog.lua',
		},
		{
			title = LOC "$$$/Photometoria/Menu/RemovePhotos=Remove Photos",
			file = 'RemovePhotosDialog.lua',
			enabledWhen = 'photosSelected',
		},
		{
			title = LOC "$$$/Photometoria/Menu/Tasks=Manage Tasks",
			file = 'TaskDialog.lua',
		},
	},

	VERSION = { major=15, minor=1, revision=0, build="20260221101", },
}
