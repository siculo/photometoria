-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local PhotoSelection = {}

--- Computes the intersection of Lightroom-selected photos and task photos.
--- selectedUuids: array of UUID strings (Lightroom photo UUIDs / client_ids).
--- taskPhotos: array of { client_id, photo_id, ... } as returned by the server.
--- Returns an array of server-side photo_id strings for the matched photos.
--- Returns an empty array when there is no overlap.
function PhotoSelection.intersect(selectedUuids, taskPhotos)
    local taskPhotoMap = {}
    for _, p in ipairs(taskPhotos) do
        if p.client_id then
            taskPhotoMap[p.client_id] = p.photo_id
        end
    end

    local result = {}
    for _, uuid in ipairs(selectedUuids) do
        local photoId = taskPhotoMap[uuid]
        if photoId then
            result[#result + 1] = photoId
        end
    end
    return result
end

return PhotoSelection
