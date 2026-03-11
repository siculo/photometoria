-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors

local TaskUtils = {}

--- Finds the index of a task with the given task_id, or nil if not found.
function TaskUtils.findTaskIndex(tasks, taskId)
	if not taskId then return nil end
	for i, task in ipairs(tasks) do
		if task.task_id == taskId then
			return i
		end
	end
	return nil
end

return TaskUtils
