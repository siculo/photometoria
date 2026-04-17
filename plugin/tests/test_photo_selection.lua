-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

--- Unit tests for PhotoSelection.lua
--- Run with: lua plugin/tests/test_photo_selection.lua

local script_dir = arg[0]:match('(.*[/\\])') or './'
package.path = script_dir .. '../photometoria.lrdevplugin/?.lua;'
            .. script_dir .. '?.lua;'
            .. package.path

local PhotoSelection = require 'PhotoSelection'
local T = require 'testkit'

local test           = T.test
local assertEqual    = T.assertEqual
local assertNil      = T.assertNil

T.suite('PhotoSelection.intersect')

test('returns matching photo_ids in selection order', function()
    local uuids = { 'uuid-1', 'uuid-2', 'uuid-99' }
    local taskPhotos = {
        { client_id = 'uuid-1', photo_id = 'pid-1' },
        { client_id = 'uuid-2', photo_id = 'pid-2' },
        { client_id = 'uuid-3', photo_id = 'pid-3' },
    }
    local result = PhotoSelection.intersect(uuids, taskPhotos)
    assertEqual(#result, 2)
    assertEqual(result[1], 'pid-1')
    assertEqual(result[2], 'pid-2')
end)

test('returns empty array when no overlap', function()
    local uuids = { 'uuid-99', 'uuid-88' }
    local taskPhotos = {
        { client_id = 'uuid-1', photo_id = 'pid-1' },
    }
    local result = PhotoSelection.intersect(uuids, taskPhotos)
    assertEqual(#result, 0)
end)

test('returns empty array for empty selected list', function()
    local taskPhotos = {
        { client_id = 'uuid-1', photo_id = 'pid-1' },
    }
    local result = PhotoSelection.intersect({}, taskPhotos)
    assertEqual(#result, 0)
end)

test('returns empty array for empty task photos', function()
    local result = PhotoSelection.intersect({ 'uuid-1' }, {})
    assertEqual(#result, 0)
end)

test('skips task photos without client_id', function()
    local uuids = { 'uuid-1' }
    local taskPhotos = {
        { client_id = nil,      photo_id = 'pid-1' },
        { client_id = 'uuid-1', photo_id = 'pid-2' },
    }
    local result = PhotoSelection.intersect(uuids, taskPhotos)
    assertEqual(#result, 1)
    assertEqual(result[1], 'pid-2')
end)

test('does not duplicate when uuid appears twice in selection', function()
    local uuids = { 'uuid-1', 'uuid-1' }
    local taskPhotos = {
        { client_id = 'uuid-1', photo_id = 'pid-1' },
    }
    local result = PhotoSelection.intersect(uuids, taskPhotos)
    assertEqual(#result, 2)
    assertEqual(result[1], 'pid-1')
    assertEqual(result[2], 'pid-1')
end)

T.report()
