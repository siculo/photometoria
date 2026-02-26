-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

return {
	LrSdkVersion = 10.0,
	LrSdkMinimumVersion = 4.0,

	LrPluginName = LOC "$$$/PluginInfo/Name=Photometoria Plugin",
	LrToolkitIdentifier = 'photometoria.plugin.lr',
	-- sdkDeprecation.action=log,

	LrPluginInfoProvider = 'PluginInfoProvider.lua',

	VERSION = { major=15, minor=1, revision=0, build="20260221101", },
}
