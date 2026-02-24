# Photometoria – Lightroom Classic Plugin Interface

## Overview

The plugin consists of three main windows (modeless dialogs) plus two system modal dialogs used for destructive operations. Navigation between main windows follows a natural flow: starting from Server Setup, moving to Task management, and from there opening the photo addition window. Modals appear in response to specific user actions without leaving the current context.

```
Setup Server → Task (master-detail) → [modal] Create Job
                                     → [modal] Confirm Task Deletion
                                     → [modal] Confirm Job Cancellation
                    ↘
                  Add Photos
```

---

## Window 1 – Server Setup

The plugin's entry point. Allows configuring the server address and viewing its status.

### Connection Section

A free-text field collects the server host and port in the format `host:port` (e.g. `192.168.1.50:8080`). The value is saved in Lightroom preferences between sessions.

Next to the field a **Verify** button appears. The button remains disabled while the field is empty; it becomes enabled as soon as something is typed.

When Verify is clicked the plugin attempts a connection to the server. While waiting, a "Verifying…" status pill appears. The outcome produces two distinct behaviors:

- **Successful connection**: green "Online" pill, the Server Details section appears below, the Save and "Go to Tasks" buttons become enabled.
- **Failed connection**: red "Unreachable" pill, the details remain hidden, Save and "Go to Tasks" stay disabled.

If the user modifies the host field after a successful verification, the state resets and the details are hidden again, requiring a new verification. This prevents saving configurations that are no longer valid.

### Server Details Section

Visible only after a successful verification. Shows information returned by the server:

- **Allocated storage**: horizontal bar with numerical labels (allocated GB out of total GB).
- **Storage used by photos**: second bar showing the actual usage of uploaded data.
- **Available providers**: list of providers configured on the server, shown as text chips.
- **Default provider**: highlighted with accent color among the provider chips.
- **Server version**, **number of active tasks**, **queued jobs**: text values.

### Buttons

- **Cancel**: closes without saving.
- **Go to Tasks**: navigates to the Task window (enabled only after successful verification).
- **Save**: saves the host in preferences (enabled only after successful verification).

---

## Window 2 – Tasks

The plugin's main window. Combines the task list (left column) and the details of the selected task (right column) in a single panel, following the master-detail pattern.

### Left Column – Task List

Each list item shows the task name, a brief summary (number of photos and size in GB) and a colored pill indicating the current status:

| Status | Meaning |
|--------|---------|
| Orange "Active" | At least one job in progress |
| Green "Completed" | All jobs finished successfully |
| Red "Errors" | Jobs finished with failed photos |

Selecting a task immediately updates the right panel without additional navigation.

At the top of the column is the **+ Add photos** button, which opens the Add Photos window. At the bottom of the column:

- **Show in Library**: selects in the Lightroom catalog the photos belonging to the selected task.
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
| Retry Failed | Job completed with at least one failed photo |
| Apply Tags to Photos | Job completed successfully |
| Cancel Job | Job in "In progress" state |
| + New Job | Always available |

---

## Window 3 – Add Photos

Opened via the "+ Add photos" button in the Task window. Guides the user in choosing which photos to add and to which task.

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

- **Cancel**: closes without doing anything.
- **Confirm and Go to Task**: creates the task (if new) or adds the photos to the existing task, then navigates directly to the Task window with the relevant task already selected. Remains disabled while in "New task" mode and the Name field is empty.

---

## Modal – Create Job

Opened via the "+ New Job" button in the Task window.

### Model selection

Two cascading dropdown menus: the first selects the provider, the second shows the available models for that provider (the list updates dynamically when the provider changes).

For cloud providers (e.g. OpenAI, Anthropic) an informational row appears showing the estimated cost for analyzing the entire photo batch, calculated before starting.

### Options

A checkbox allows enabling **automatic tag application** at the end of the job: if selected, upon completion the tags are written to the Lightroom Keywords field without requiring further user action.

### Summary

A panel at the bottom of the modal shows, in read-only form, a summary of the configuration before starting: number of photos, selected model, status of the auto-apply option.

### Buttons

- **Cancel**: closes without creating the job.
- **▶ Start Job**: creates the job and queues it on the server.

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

## Design Notes

**Master-detail pattern for the Task window.** The choice to combine the task list and detail in a single window stems from the observation that the two separate screens were too tightly coupled to live independently: the back-and-forth navigation with the "Back to Tasks" button was a signal of excessive coupling. The master-detail pattern allows always seeing the full context without changing screens.

**Confirmations before destructive operations.** Both task deletion and job cancellation require explicit confirmation via a dedicated modal. The text of the confirmation buttons is deliberately descriptive ("Delete permanently", "Cancel the job") to reduce the risk of accidental clicks. The deletion modal uses red, the job cancellation modal uses amber, because the consequences are asymmetric: deletion is irreversible, cancellation leaves partial results available.

**Conditional server details.** Server information is shown only after an explicit connection verification. This avoids showing potentially stale data and makes it clear to the user when the configuration has not yet been validated.

**Confirm button disabled without task name.** In the add photos window, the confirm button is disabled while in "New task" mode and the Name field is empty. The Context field is not blocking because it can be filled in later from the Task window.

**Automatic tag application as opt-in.** The checkbox in job creation is unchecked by default: writing metadata to the Lightroom catalog is a significant action and the user may want to review the results first. Those who want the automatic behavior can enable it explicitly.

---

*Document updated in conjunction with the `PLUGIN_UI_PROTOTYPE.html` prototype.*
