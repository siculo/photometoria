# UI Prototype Corrections

## Menu

The tabs at the top should be replaced with a menu providing access to three windows corresponding to the current "setup server", "task", and "add photos" panels. The menu can be presented as a simple list of items.

The windows are initially closed and are opened when the corresponding menu item is selected.

The menu items are:
- Photomemoria Setup
- Task
- Add Photos

Each menu item opens the corresponding window.

The "Task" and "Add Photos" items are enabled only if the server is correctly configured.

## Photomemoria Setup Window

The "host:port" field must be validated, and the host can be either an IP address or a hostname (e.g., "localhost").

The "Verify" button becomes "Connect".

The "Save" button is enabled only if the connection to the server is active, after pressing the "Connect" button, and if the host and port have been modified.

The "Go to Task" button should be removed.

Pressing the "Cancel" or "Save" buttons closes the window.

## Task Window

The "Add photos" button should be removed.

The "Show in Library" button becomes "Show Photos in Library". This button closes the window and selects all photos in the task in the library. In this prototype, since there is no library, this last operation is not performed and the window is simply closed.

The "Retry Failed" and "Apply Tags" buttons must also open a confirmation alert.

The "New Job" and "Retry Failed" buttons must add a new job to the job list.

A button to remove completed jobs is needed.

A window close button is needed.

## Add Photos Window

The "Confirm and go to task" button must close the "Add Photos" window and open the "Task" window, where the new task must be added to the task list.
