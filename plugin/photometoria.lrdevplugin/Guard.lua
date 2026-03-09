-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

--- Reentrancy guard for scripts executed via LrLibraryMenuItems.
--- State persists across invocations because `require` caches modules.

local Guard = {}

local locks = {}

--- Attempts to acquire a named lock. Returns true if acquired, false if busy.
function Guard.acquire(name)
	if locks[name] then
		return false
	end
	locks[name] = true
	return true
end

--- Releases a named lock.
function Guard.release(name)
	locks[name] = nil
end

return Guard
