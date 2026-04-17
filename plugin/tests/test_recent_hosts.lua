-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

package.path = 'plugin/photometoria.lrdevplugin/?.lua;plugin/tests/?.lua;' .. package.path

local testkit = require 'testkit'

-- Stub the Lightroom SDK so RecentHosts can run outside Lightroom.
local prefs = {}
function import(name)
    if name == 'LrPrefs' then
        return {
            prefsForPlugin = function() return prefs end,
        }
    end
    error('unexpected import: ' .. name)
end

local RecentHosts = require 'RecentHosts'

local function reset()
    prefs = {}
end

testkit.suite('RecentHosts tests')

testkit.test('load returns empty table when nothing stored', function()
    reset()
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 0)
end)

testkit.test('add stores host with name', function()
    reset()
    RecentHosts.add('192.168.1.50:8080', 'Studio NAS')
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 1)
    testkit.assertEqual(entries[1].host, '192.168.1.50:8080')
    testkit.assertEqual(entries[1].name, 'Studio NAS')
end)

testkit.test('add stores host without name', function()
    reset()
    RecentHosts.add('localhost:3000')
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 1)
    testkit.assertEqual(entries[1].host, 'localhost:3000')
    testkit.assertEqual(entries[1].name, '')
end)

testkit.test('add moves duplicate to front and updates name', function()
    reset()
    RecentHosts.add('host-a:8080', 'Old Name')
    RecentHosts.add('host-b:8080', 'Server B')
    RecentHosts.add('host-a:8080', 'New Name')
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 2)
    testkit.assertEqual(entries[1].host, 'host-a:8080')
    testkit.assertEqual(entries[1].name, 'New Name')
    testkit.assertEqual(entries[2].host, 'host-b:8080')
end)

testkit.test('load is backward compatible with name-less entries', function()
    reset()
    prefs['recentHosts'] = 'oldhost:9000|anotherhost:1234'
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 2)
    testkit.assertEqual(entries[1].host, 'oldhost:9000')
    testkit.assertEqual(entries[1].name, '')
    testkit.assertEqual(entries[2].host, 'anotherhost:1234')
    testkit.assertEqual(entries[2].name, '')
end)

testkit.test('toComboItems returns display string with name', function()
    reset()
    RecentHosts.add('192.168.1.50:8080', 'Studio NAS')
    RecentHosts.add('localhost:3000', '')
    local items = RecentHosts.toComboItems('Clear list')
    testkit.assertEqual(#items, 3)
    testkit.assertEqual(items[1], 'localhost:3000')
    testkit.assertEqual(items[2], '192.168.1.50:8080 - Studio NAS')
    testkit.assertEqual(items[3], 'Clear list')
end)

testkit.test('toComboItems omits clear label when list is empty', function()
    reset()
    local items = RecentHosts.toComboItems('Clear list')
    testkit.assertEqual(#items, 0)
end)

testkit.test('extractHost strips name suffix', function()
    reset()
    testkit.assertEqual(RecentHosts.extractHost('192.168.1.50:8080 - Studio NAS'), '192.168.1.50:8080')
end)

testkit.test('extractHost returns plain host unchanged', function()
    reset()
    testkit.assertEqual(RecentHosts.extractHost('localhost:3000'), 'localhost:3000')
end)

testkit.test('extractHost handles name containing hyphen', function()
    reset()
    testkit.assertEqual(RecentHosts.extractHost('10.0.0.1:8080 - My-Server'), '10.0.0.1:8080')
end)

testkit.test('clear removes all entries', function()
    reset()
    RecentHosts.add('host-a:8080', 'A')
    RecentHosts.add('host-b:8080', 'B')
    RecentHosts.clear()
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 0)
end)

testkit.test('clear keeps one entry when keepHost provided', function()
    reset()
    RecentHosts.add('host-a:8080', 'A')
    RecentHosts.add('host-b:8080', 'B')
    RecentHosts.clear('host-a:8080')
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 1)
    testkit.assertEqual(entries[1].host, 'host-a:8080')
end)

testkit.test('clear extracts host from display string', function()
    reset()
    RecentHosts.add('host-a:8080', 'A')
    RecentHosts.clear('host-a:8080 - A')
    local entries = RecentHosts.load()
    testkit.assertEqual(#entries, 1)
    testkit.assertEqual(entries[1].host, 'host-a:8080')
end)

testkit.report()
