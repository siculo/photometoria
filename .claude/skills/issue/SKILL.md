---
name: issue
description: >
  Start working on a GitHub issue. Fetches the issue details and automatically
  activates the relevant development skills based on issue labels. Use when the
  user says "work on issue #N", "start issue N", or invokes /issue <number>.
  Requires a GitHub issue number as argument.
---

# Work on GitHub Issue

You have been asked to start working on a GitHub issue. Follow these steps:

## Step 1 — Fetch the issue

Run this command to get the issue details (replace `<number>` with the argument):

```bash
gh issue view <number> --json number,title,body,state,labels,assignees
```

If the command fails, inform the user and stop.

## Step 2 — Activate component skills based on labels

Inspect the `labels` array from the response. For each label whose `name`
matches a component tag, invoke the corresponding skill using the Skill tool:

| Label name         | Skill to invoke |
|--------------------|-----------------|
| `component: api`   | `api-dev`       |
| `component: plugin`| `plugin-dev`    |

Invoke all matching skills. If **neither** label is present, skip this step
(the user may activate skills manually if needed).

## Step 3 — Create the feature branch (if needed)

Check the current git branch. If you are NOT already on a branch that
references this issue number (e.g., `feature/<number>-*`), ask the user
whether they want to create or switch to one.

## Step 4 — Present the issue summary

After activating the skills, present the issue to the user in this format:

```
## Issue #<number>: <title>

**State:** <state>
**Labels:** <comma-separated label names>
**Assignees:** <comma-separated assignee logins, or "none">

<issue body, rendered as markdown>
```

Then ask the user how they would like to proceed with the implementation.
