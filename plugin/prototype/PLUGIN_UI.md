# Photometoria – Lightroom Classic Plugin Interface

## Overview

The plugin consists of three main windows (modeless dialogs) plus seven modal dialogs used for destructive or significant operations. A horizontal menu bar at the top provides access to the three windows; each window can be opened and closed independently. Modals appear in response to specific user actions without leaving the current context.

The prototype was originally designed without Lightroom SDK constraints in mind,
featuring rich interactive elements (dynamic lists, progress bars, modals) that are
fully achievable in a web context but only partially reproducible within the
Lightroom plugin SDK.

## Files

- `PLUGIN_UI_PROTOTYPE.html` — Interactive HTML/JS prototype of the full plugin UI

## Context

The Lightroom plugin implementation adapts these designs to work within the SDK's
limitations (see `plugin/CLAUDE.md` for the constraint catalog). A web-based interface
for the Photometoria server could implement the original vision without those
restrictions.

### Navigation Menu

A horizontal menu bar is always visible at the top of the interface. It contains a "Photometoria" brand label followed by three items:

- **Photomemoria Setup**: opens the server configuration window.
- **Add Photos**: opens the photo addition window.
- **Task**: opens the task management window.

The **Add Photos** and **Task** items are disabled until the server is correctly configured (i.e. a successful connection has been established). All windows start closed; selecting a menu item opens the corresponding window and closes any other open window.

```
[Photometoria]  Photomemoria Setup | Add Photos | Task

  Photomemoria Setup
  Add Photos
  Task (dropdown + detail)  →  [modal] Create Job
                             →  [modal] Confirm Task Deletion
                             →  [modal] Confirm Job Cancellation
                             →  [modal] Confirm Retry Failed
                             →  [modal] Confirm Restart
                             →  [modal] Confirm Job Removal
                             →  [modal] Confirm Apply Tags
                             →  [modal] Remove After Apply
```

---

## Window 1 – Photomemoria Setup

The plugin's entry point. Allows configuring the server address and viewing its status.

### Connection Section

A free-text field collects the server host and port in the format `host:port` (e.g. `192.168.1.50:8080` or `localhost:8080`). The host can be either an IP address or a hostname. The value is saved in Lightroom preferences between sessions.

The field is validated in real time: if the format is invalid, a validation error message appears below the field and the input is highlighted in red. The **Connect** button remains disabled while the field is empty or contains an invalid value; it becomes enabled as soon as a valid host:port is entered.

When Connect is clicked the plugin attempts a connection to the server. While waiting, a "[⟳ Connecting…]" status label appears. The outcome produces two distinct behaviors:

- **Successful connection**: "[✓ Online]" label with supplementary text "Connection successful", the Server Details section appears below, and the Add Photos and Task menu items become enabled.
- **Failed connection**: "[✕ Unreachable]" label with supplementary text "No response from server", the details remain hidden.

If the user modifies the host field after a successful connection, the state resets and the details are hidden again, requiring a new connection. This prevents saving configurations that are no longer valid.

### Server Details Section

Visible only after a successful connection. Shows information returned by the server:

- **Allocated storage**: horizontal bar with numerical labels (allocated GB out of total GB).
- **Used storage (photos)**: second bar showing the actual usage of uploaded data.
- A summary text line below the bars (e.g. "62.4 GB allocated · 23.5 GB actually used by photos · 76.5 GB available").
- **Providers**: list of providers configured on the server, shown as text chips.
- **Default**: shown as a separate field row with a chip containing both the provider name and default model (e.g. "Ollama → qwen2-vl:8b").
- **Version**: server version string.
- **Active tasks**: number of active tasks.
- **Queued jobs**: number of queued jobs.

### Buttons

- **Cancel**: closes the window without saving. Resets the connection state, restores the host field to the previously saved value, and hides server details.
- **Save**: saves the host in preferences and closes the window. Enabled only if the connection to the server is active (after pressing Connect) and the host:port has been modified compared to the previously saved value.

---

## Window 2 – Tasks

The plugin's main window. Uses a task dropdown at the top and a detail area below.

### Task Selector Row

A dropdown (`popup_menu`) at the top allows selecting the active task. Each dropdown item shows the task name followed by text-based status icons representing the state of its jobs: `✓` completed, `⚠` errors, `✕` cancelled, `⟳` running.

The task selector row has a thicker bottom border to visually separate it from the detail area below. To the right of the dropdown:

- **Delete**: removes the task and all its data from the server. Disabled if the task has active jobs.

### Detail Area

The detail area is divided vertically into three stacked sections: Context, Task Info, and Jobs.

**Context Section**

An editable text area contains the task description, i.e. the context information the model will use during photo analysis (location, event, period, style, etc.).

The **Save** and **Cancel** buttons appear below the field only when the text is modified, and disappear after saving or cancelling. Cancel restores the previously saved text.

**Task Info Row**

A compact row shows the selected task's metadata: number of photos, size in GB, and number of jobs (e.g. "72 photos · 1.8 GB · 3 jobs"). When the task has active jobs and deletion is disabled, an explanatory text appears here in red: "Task with active job — cannot delete". The **Show Photos in Library** button is aligned to the right of this row; it selects the task's photos in the Lightroom catalog, then closes the window.

**Jobs Section**

The jobs area follows a master-detail pattern with two side-by-side columns:

*Left column – Job list (240px)*

A scrollable list of jobs for the selected task. Each job item shows minimal text: the job name (provider · model) and a text-based status icon. The "**+ Start New Job**" button is at the bottom of this column, always available.

*Right column – Job detail*

Shows the details of the selected job:

- Job name (provider · model) and a text status label with icon (e.g. "[⟳ In progress]", "[✓ Completed]", "[⚠ With errors]", "[✕ Cancelled]").
- Progress bar with photo counter and estimated remaining time for running jobs; or final summary (total photos, time taken, any errors) for completed jobs.

Action buttons are shown/hidden based on the selected job's state:

| Button | Activation condition |
|--------|---------------------|
| ✓ Apply Tags | Job completed, with or without errors (opens confirmation modal; after applying, a follow-up modal asks whether to remove the job) |
| ↻ Retry Failed | Job completed with at least one failed photo (opens confirmation modal, then adds a new retry job) |
| ▶ Restart | Job cancelled (opens confirmation modal, then adds a new job that reprocesses all photos) |
| ✕ Cancel | Job in "In progress" state (opens confirmation modal) |
| 🗑 Remove | Job not in "In progress" state (opens confirmation modal for individual job removal) |

### Bottom Buttons

- **Close**: closes the Tasks window.

---

## Window 3 – Add Photos

Opened from the menu. Guides the user in choosing which photos to add and to which task.

### Layout

Uses a two-column layout: left column contains the radio groups for photo and destination selection, right column shows the contextual form (new task fields or existing task picker).

### Photo selection (left column)

Two radio options:

- **Selected only** (default): adds only the photos selected in Lightroom at the time of opening.
- **All**: adds all photos from the active catalog or collection.

Below the options, a summary box shows the number of photos that would be added and the estimated size (e.g. "47 photos selected · 1.2 GB estimated").

### Destination selection (left column)

Two alternative radio options:

**New task** (right column shows):
- *Name* (required): task identifier in the plugin.
- *Context* (optional): description of the photos to orient the model.
- An informational note: "Context helps the model generate more precise and contextual tags."

**Existing task** (right column shows):
- A dropdown menu with the tasks present on the server.
- A summary box showing the number of photos already in the selected task, the current size, and the estimated count and size after addition.

### Buttons

- **Cancel**: closes the window without doing anything.
- **Confirm and Go to Task →**: creates the task (if new) or adds the photos to the existing task, then closes the Add Photos window and opens the Task window with the relevant task already selected. Remains disabled while in "New task" mode and the Name field is empty.

---

## Modal – Create Job

Opened via the "+ Start New Job" button in the Task window. The title bar shows "New Job – {task name}". The new job is added to the job list of the currently selected task.

### Model selection

Two cascading dropdown menus: the first selects the provider, the second shows the available models for that provider (the list updates dynamically when the provider changes).

For cloud providers (e.g. OpenAI, Anthropic) an informational row appears showing the estimated cost for analyzing the entire photo batch, calculated before starting (e.g. "Estimated cost: ~€ 0.72 for 72 photos").

### Summary

A panel at the bottom of the modal shows, in read-only form, a summary of the configuration before starting: number of photos to process and selected model.

### Buttons

- **Cancel**: closes without creating the job.
- **▶ Start Job**: creates the job, adds it to the job list, and queues it on the server.

---

## Modal – Confirm Task Deletion

Opened from the Delete button in the Task window. Title bar: "Confirm deletion".

Shows a message in the form: `Delete task "{task name}"?` followed by the warning text: "All task data will be deleted from the server: jobs and uploaded photos. Original photos in Lightroom will not be affected. This operation is irreversible."

**Buttons:**
- **Cancel**: closes without deleting.
- **Delete** (danger style): proceeds with the deletion.

---

## Modal – Confirm Job Cancellation

Opened from the Cancel button in the Task window. Title bar: "Confirm cancellation".

Shows a message in the form: `Cancel job "{job name}"?` followed by: "The job will be stopped after the current photo finishes processing. Partial results already obtained will remain available."

**Buttons:**
- **Back**: closes without cancelling (the term "Back" is intentional to avoid ambiguity with "Cancel the job").
- **Cancel Job** (danger style): proceeds with the interruption.

---

## Modal – Confirm Retry Failed

Opened from the Retry Failed button in the Task window. Title bar: "Confirm retry".

Shows a message in the form: `Retry failed photos from "{job name}"?` followed by: "The failed photos will be reprocessed using the same model and settings."

**Buttons:**
- **Cancel**: closes without retrying.
- **Retry** (primary style): creates a new retry job and adds it to the job list.

---

## Modal – Confirm Job Removal

Opened from the Remove button in the job detail panel. Title bar: "Confirm removal".

Shows a message in the form: `Remove job "{job name}"?` followed by: "The job and its results will be removed from this task."

**Buttons:**
- **Cancel**: closes without removing.
- **Remove** (danger style): removes the individual job from the task.

---

## Modal – Confirm Restart

Opened from the Restart button in the job detail panel. Title bar: "Confirm restart".

Shows a message in the form: `Restart job "{job name}"?` followed by: "All photos will be reprocessed using the same model and settings."

**Buttons:**
- **Cancel**: closes without restarting.
- **Restart** (primary style): creates a new job that reprocesses all photos and adds it to the job list.

---

## Modal – Confirm Apply Tags

Opened from the Apply Tags button in the Task window. Title bar: "Confirm apply tags".

Shows a message in the form: `Apply tags from "{job name}"?` followed by: "Tags will be written to the Lightroom Keywords field. This action will modify your catalog metadata."

**Buttons:**
- **Cancel**: closes without applying.
- **Apply Tags** (primary style): proceeds with writing tags to the catalog, then opens the Remove After Apply modal.

---

## Modal – Remove After Apply

Opened automatically after tags have been applied successfully. Title bar: "Tags applied".

Shows a message in the form: `Tags from "{job name}" have been applied successfully.` followed by: "Do you want to remove this job from the task?"

**Buttons:**
- **Keep Job**: closes the modal; the job remains in the list (without the Apply Tags button, since tags have already been applied).
- **Remove Job** (danger style): removes the job from the task.

---

## Design Notes

**Horizontal menu bar for navigation.** The three windows are accessed through a horizontal menu bar at the top, prefixed by the "Photometoria" brand label. The menu items for Add Photos and Task are disabled until the server is configured, enforcing the correct workflow order.

**Task dropdown with detail panel.** The task list uses a dropdown (`popup_menu`) rather than a scrollable side column, consistent with LrView SDK's available list widgets. The dropdown is visually separated from the detail area below with a thicker border. Task metadata (photo count, size, job count) is shown in an info row within the detail area, after the context field. This approach maps directly to the `popup_menu` widget available in the SDK.

**Nested master-detail for jobs.** Within the task detail area, jobs use their own master-detail layout: a narrow list on the left and a detail panel on the right. This allows showing detailed job information (progress, actions) without cluttering the job list items, which remain minimal text-only entries compatible with `simple_list`.

**Per-job action buttons.** Action buttons appear in the job detail panel for the selected job, shown or hidden based on the job's state. This replaces the earlier design of bulk actions at the bottom of the job column.

**Per-job removal instead of bulk removal.** The "Remove" button removes an individual job (with confirmation), replacing the earlier "Remove Completed" bulk action. This provides more granular control.

**Retry vs Restart.** Two distinct actions handle re-execution: "Retry Failed" reprocesses only the failed photos from a job that completed with errors; "Restart" reprocesses all photos from a cancelled job. This distinction reflects the different semantics: errors produce partial valid results, while cancellation interrupts processing entirely.

**Two-step Apply Tags flow.** Applying tags is followed by a second modal asking whether to remove the job. This avoids the automatic deletion of the previous design: the user may want to keep the job as a reference (to see which model generated the tags, review metadata, etc.).

**Confirmations before destructive and significant operations.** Task deletion, job cancellation, job removal, retry failed, restart, and apply tags all require explicit confirmation via a dedicated modal. Destructive actions (delete, cancel, remove) use danger-styled buttons; non-destructive significant actions (retry, restart, apply tags) use primary-styled buttons.

**Host:port validation.** The host:port field is validated in real time to accept both IP addresses and hostnames. Invalid input is highlighted and the Connect button remains disabled, preventing connection attempts with malformed addresses.

**Conditional server details.** Server information is shown only after an explicit connection. This avoids showing potentially stale data and makes it clear to the user when the configuration has not yet been validated.

**Save enabled only when meaningful.** The Save button in the setup window is enabled only when both conditions are met: the server connection is active and the host:port value differs from the previously saved one. This prevents unnecessary saves and makes it clear when a change has actually been made.

**Confirm button disabled without task name.** In the add photos window, the confirm button is disabled while in "New task" mode and the Name field is empty. The Context field is not blocking because it can be filled in later from the Task window.

---

*Document updated in conjunction with the `PLUGIN_UI_PROTOTYPE.html` prototype.*
