-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local LrPrefs = import 'LrPrefs'

local MAX_RECENT = 10
local PREFS_KEY = 'recentHosts'

local RecentHosts = {}

--- Returns an array of recently connected host strings, most recent first.
function RecentHosts.load()
    local prefs = LrPrefs.prefsForPlugin()
    local stored = prefs[PREFS_KEY]
    if type(stored) ~= 'string' or stored == '' then
        return {}
    end
    local hosts = {}
    for host in stored:gmatch('[^|]+') do
        hosts[#hosts + 1] = host
    end
    return hosts
end

local function save(hosts)
    local prefs = LrPrefs.prefsForPlugin()
    prefs[PREFS_KEY] = table.concat(hosts, '|')
end

--- Adds a host to the front of the recent list, removing any duplicate entry
--- and capping the list at MAX_RECENT entries.
function RecentHosts.add(host)
    local hosts = RecentHosts.load()
    for i = #hosts, 1, -1 do
        if hosts[i] == host then
            table.remove(hosts, i)
        end
    end
    table.insert(hosts, 1, host)
    while #hosts > MAX_RECENT do
        table.remove(hosts)
    end
    save(hosts)
end

--- Clears the recent hosts list, optionally keeping a single host entry.
function RecentHosts.clear(keepHost)
    if keepHost and keepHost ~= '' then
        save({ keepHost })
    else
        save({})
    end
end

--- Returns the host list as a flat string array for use as combo_box items.
--- When the list is non-empty, appends clearLabel as the last entry so the user
--- can remove all hosts directly from the dropdown.
function RecentHosts.toComboItems(clearLabel)
    local hosts = RecentHosts.load()
    local items = {}
    for _, host in ipairs(hosts) do
        items[#items + 1] = host
    end
    if #hosts > 0 then
        items[#items + 1] = clearLabel
    end
    return items
end

return RecentHosts
