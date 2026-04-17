-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrPrefs = import 'LrPrefs'

local MAX_RECENT = 10
local PREFS_KEY = 'recentHosts'
local NAME_SEP = '\1'

local RecentHosts = {}

--- Returns an array of {host, name} tables, most recent first.
--- Backward compatible: entries without NAME_SEP have name = ''.
function RecentHosts.load()
    local prefs = LrPrefs.prefsForPlugin()
    local stored = prefs[PREFS_KEY]
    if type(stored) ~= 'string' or stored == '' then
        return {}
    end
    local entries = {}
    for entry in stored:gmatch('[^|]+') do
        local sep = entry:find(NAME_SEP, 1, true)
        if sep then
            entries[#entries + 1] = { host = entry:sub(1, sep - 1), name = entry:sub(sep + 1) }
        else
            entries[#entries + 1] = { host = entry, name = '' }
        end
    end
    return entries
end

local function save(entries)
    local prefs = LrPrefs.prefsForPlugin()
    local parts = {}
    for _, e in ipairs(entries) do
        parts[#parts + 1] = e.host .. NAME_SEP .. e.name
    end
    prefs[PREFS_KEY] = table.concat(parts, '|')
end

--- Adds or updates a host entry. If the host already exists, moves it to the
--- front and updates the name. Caps the list at MAX_RECENT entries.
function RecentHosts.add(host, name)
    local entries = RecentHosts.load()
    for i = #entries, 1, -1 do
        if entries[i].host == host then
            table.remove(entries, i)
        end
    end
    table.insert(entries, 1, { host = host, name = name or '' })
    while #entries > MAX_RECENT do
        table.remove(entries)
    end
    save(entries)
end

--- Clears the recent hosts list, optionally keeping a single host entry.
function RecentHosts.clear(keepHost)
    if keepHost and keepHost ~= '' then
        local host = RecentHosts.extractHost(keepHost)
        save({ { host = host, name = '' } })
    else
        save({})
    end
end

--- Extracts the host:port from a display string that may contain a server name.
--- "192.168.1.50:8080 - Studio NAS" → "192.168.1.50:8080"
--- "192.168.1.50:8080"              → "192.168.1.50:8080"
function RecentHosts.extractHost(value)
    if type(value) ~= 'string' then return '' end
    return value:match('^(%S+) %- ') or value
end

--- Returns the host list as display strings for use as combo_box items.
--- Each entry is formatted as "host - name" when a name is available, or
--- just "host" otherwise. Appends clearLabel when the list is non-empty.
function RecentHosts.toComboItems(clearLabel)
    local entries = RecentHosts.load()
    local items = {}
    for _, e in ipairs(entries) do
        if e.name and e.name ~= '' then
            items[#items + 1] = e.host .. ' - ' .. e.name
        else
            items[#items + 1] = e.host
        end
    end
    if #entries > 0 then
        items[#items + 1] = clearLabel
    end
    return items
end

return RecentHosts
