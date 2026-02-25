# Photometoria – Lightroom Classic Plugin Interface

## Overview

The plugin consists of three main windows (modeless dialogs) plus four modal dialogs used for destructive or significant operations. A vertical menu on the left provides access to the three windows; each window can be opened and closed independently. Modals appear in response to specific user actions without leaving the current context.

### Navigation Menu

A vertical menu is always visible on the left side of the interface. It contains three items:

- **Photomemoria Setup**: opens the server configuration window.
- **Task**: opens the task management window.
- **Add Photos**: opens the photo addition window.

The **Task** and **Add Photos** items are disabled until the server is correctly configured (i.e. a successful connection has been established). All windows start closed; selecting a menu item opens the corresponding window and closes any other open window.

```
[Menu]  →  Photomemoria Setup
        →  Task (master-detail)  →  [modal] Create Job
                                 →  [modal] Confirm Task Deletion
                                 →  [modal] Confirm Job Cancellation
                                 →  [modal] Confirm Retry Failed
                                 →  [modal] Confirm Apply Tags
        →  Add Photos
```

---

## Window 1 – Photomemoria Setup

The plugin's entry point. Allows configuring the server address and viewing its status. Each window has a close button (×) in the title bar.

### Connection Section

A free-text field collects the server host and port in the format `host:port` (e.g. `192.168.1.50:8080` or `localhost:8080`). The host can be either an IP address or a hostname. The value is saved in Lightroom preferences between sessions.

The field is validated in real time: if the format is invalid, a validation error message appears below the field and the input is highlighted in red. The **Connect** button remains disabled while the field is empty or contains an invalid value; it becomes enabled as soon as a valid host:port is entered.

When Connect is clicked the plugin attempts a connection to the server. While waiting, a "Connecting…" status pill appears. The outcome produces two distinct behaviors:

- **Successful connection**: green "Online" pill, the Server Details section appears below, and the Task and Add Photos menu items become enabled.
- **Failed connection**: red "Unreachable" pill, the details remain hidden.

If the user modifies the host field after a successful connection, the state resets and the details are hidden again, requiring a new connection. This prevents saving configurations that are no longer valid.

### Server Details Section

Visible only after a successful connection. Shows information returned by the server:

- **Allocated storage**: horizontal bar with numerical labels (allocated GB out of total GB).
- **Storage used by photos**: second bar showing the actual usage of uploaded data.
- **Available providers**: list of providers configured on the server, shown as text chips.
- **Default provider**: highlighted with accent color among the provider chips.
- **Server version**, **number of active tasks**, **queued jobs**: text values.

### Buttons

- **Cancel**: closes the window without saving.
- **Save**: saves the host in preferences and closes the window. Enabled only if the connection to the server is active (after pressing Connect) and the host:port has been modified compared to the previously saved value.

---

## Window 2 – Tasks

The plugin's main window. Combines the task list (left column) and the details of the selected task (right column) in a single panel, following the master-detail pattern. Has a close button (×) in the title bar.

### Left Column – Task List

Each list item shows the task name, a brief summary (number of photos and size in GB) and a colored pill indicating the current status:

| Status | Meaning |
|--------|---------|
| Orange "Active" | At least one job in progress |
| Green "Completed" | All jobs finished successfully |
| Red "Errors" | Jobs finished with failed photos |

Selecting a task immediately updates the right panel — including the job list — without additional navigation. The job list is dynamically rebuilt each time a different task is selected, showing only the jobs belonging to that task.

At the bottom of the column:

- **Show Photos in Library**: selects in the Lightroom catalog the photos belonging to the selected task, then closes the window.
- **Delete**: removes the task and all its data from the server. Disabled if the task has active jobs; in that case an explanatory text informs the user of the reason. The click opens the confirmation modal (see below).

### Right Column – Task Detail

Divided internally into two side-by-side parts.

**Left part – Context**

A full-height editable text area contains the task description, i.e. the context information the model will use during photo analysis (location, event, period, style, etc.).

The **Save** and **Cancel** buttons appear at the bottom of the field only when the text is modified, and disappear after saving or cancelling. Cancel restores the previously saved text.

**Right part – Jobs**

List of jobs for the selected task. For each job the following is visible:

- Provider and model used (e.g. `Ollama · qwen2-vl:8b`).
- Progress: progress bar with photo counter and estimated remaining time for running jobs; or final summary (total photos, time taken, any errors) for completed jobs.
- Status pill: In progress (orange), Completed (green), With errors (red), Cancelled (grey).

At the bottom of the job column are the action buttons, dynamically enabled based on the selected job:

| Button | Activation condition |
|--------|---------------------|
| Remove Completed | Always available (removes all completed jobs from the list) |
| Retry Failed | Job completed with at least one failed photo (opens confirmation modal, then adds a new retry job) |
| Apply Tags | Job completed successfully (opens confirmation modal) |
| Cancel Job | Job in "In progress" state |
| + New Job | Always available |

---

## Window 3 – Add Photos

Opened from the menu. Guides the user in choosing which photos to add and to which task. Has a close button (×) in the title bar.

### Photo selection

Two radio options:

- **Selected only** (default, available only if an active selection exists in the catalog): adds only the photos selected in Lightroom at the time of opening.
- **All**: adds all photos from the active catalog or collection.

Below the options, a summary box updated in real time shows the number of photos that would be added and the estimated size.

### Destination selection

Two alternative radio options:

**New task**: shows two fields:
- *Name* (required): task identifier in the plugin.
- *Context* (optional): description of the photos to orient the model.

An informational note reminds that a well-filled context improves the quality of the generated tags.

**Existing task**: shows a dropdown menu with the tasks present on the server and a summary box showing the number of photos already in the selected task and the estimated count after addition.

### Buttons

- **Cancel**: closes the window without doing anything.
- **Confirm and Go to Task**: creates the task (if new) or adds the photos to the existing task, then closes the Add Photos window and opens the Task window with the relevant task already selected and added to the task list. Remains disabled while in "New task" mode and the Name field is empty.

---

## Modal – Create Job

Opened via the "+ New Job" button in the Task window. The new job is added to the job list of the currently selected task.

### Model selection

Two cascading dropdown menus: the first selects the provider, the second shows the available models for that provider (the list updates dynamically when the provider changes).

For cloud providers (e.g. OpenAI, Anthropic) an informational row appears showing the estimated cost for analyzing the entire photo batch, calculated before starting.

### Options

A checkbox allows enabling **automatic tag application** at the end of the job: if selected, upon completion the tags are written to the Lightroom Keywords field without requiring further user action.

### Summary

A panel at the bottom of the modal shows, in read-only form, a summary of the configuration before starting: number of photos, selected model, status of the auto-apply option.

### Buttons

- **Cancel**: closes without creating the job.
- **▶ Start Job**: creates the job, adds it to the job list, and queues it on the server.

---

## Modal – Confirm Task Deletion

Opened from the Delete button in the Task window.

Shows the name of the task about to be deleted and its summary data (number of photos, size, number of jobs). A red warning states that all task data will be deleted from the server (jobs and uploaded photos) and that the original photos in the Lightroom catalog will not be affected. The operation is irreversible.

**Buttons:**
- **Cancel**: closes without deleting.
- **Delete permanently**: proceeds with the deletion.

The modal also closes when clicking outside it.

---

## Modal – Confirm Job Cancellation

Opened from the Cancel Job button in the Task window.

Shows the provider and model of the selected job and its current status. An amber warning informs that the job will be stopped after the current photo finishes processing and that partial results already obtained will remain available.

**Buttons:**
- **Back**: closes without cancelling (the term "Back" is intentional to avoid ambiguity with "Cancel the job").
- **Cancel the job**: proceeds with the interruption.

The modal also closes when clicking outside it.

---

## Modal – Confirm Retry Failed

Opened from the Retry Failed button in the Task window.

Shows the provider and model of the job whose failed photos will be retried. An informational note explains that the failed photos will be reprocessed using the same model and settings.

**Buttons:**
- **Cancel**: closes without retrying.
- **Retry Failed Photos**: creates a new retry job and adds it to the job list.

The modal also closes when clicking outside it.

---

## Modal – Confirm Apply Tags

Opened from the Apply Tags button in the Task window.

Shows the provider and model of the selected job. An amber warning informs that tags will be written to the Lightroom Keywords field, modifying catalog metadata.

**Buttons:**
- **Cancel**: closes without applying.
- **Apply Tags to Photos**: proceeds with writing tags to the catalog.

The modal also closes when clicking outside it.

---

## Design Notes

**Vertical menu for navigation.** The three windows are accessed through a vertical menu rather than tabs. This makes it clearer that the windows are independent dialogs that can be opened and closed, rather than panels of a single interface. The menu items for Task and Add Photos are disabled until the server is configured, enforcing the correct workflow order.

**Master-detail pattern for the Task window.** The choice to combine the task list and detail in a single window stems from the observation that the two separate screens were too tightly coupled to live independently: the back-and-forth navigation with the "Back to Tasks" button was a signal of excessive coupling. The master-detail pattern allows always seeing the full context without changing screens.

**Dynamic job list per task.** The job list is rebuilt each time a task is selected, ensuring that only jobs belonging to the selected task are shown. This avoids confusion when switching between tasks with different job states.

**Confirmations before destructive and significant operations.** Task deletion, job cancellation, retry failed, and apply tags all require explicit confirmation via a dedicated modal. The text of the confirmation buttons is deliberately descriptive ("Delete permanently", "Cancel the job", "Retry Failed Photos", "Apply Tags to Photos") to reduce the risk of accidental clicks. The deletion modal uses red, the job cancellation and apply tags modals use amber, because the consequences are asymmetric: deletion is irreversible, while cancellation and tag application have more limited impact.

**Host:port validation.** The host:port field is validated in real time to accept both IP addresses and hostnames. Invalid input is highlighted and the Connect button remains disabled, preventing connection attempts with malformed addresses.

**Conditional server details.** Server information is shown only after an explicit connection. This avoids showing potentially stale data and makes it clear to the user when the configuration has not yet been validated.

**Save enabled only when meaningful.** The Save button in the setup window is enabled only when both conditions are met: the server connection is active and the host:port value differs from the previously saved one. This prevents unnecessary saves and makes it clear when a change has actually been made.

**Confirm button disabled without task name.** In the add photos window, the confirm button is disabled while in "New task" mode and the Name field is empty. The Context field is not blocking because it can be filled in later from the Task window.

**Automatic tag application as opt-in.** The checkbox in job creation is unchecked by default: writing metadata to the Lightroom catalog is a significant action and the user may want to review the results first. Those who want the automatic behavior can enable it explicitly.

---

*Document updated in conjunction with the `PLUGIN_UI_PROTOTYPE.html` prototype.*
