---
name: ui-prototype
description: >
  Rapid UI prototyping as a single self-contained HTML file with constraint tracking and theme
  support. Use this skill whenever the user wants to prototype, mock up, or preview a user
  interface before implementing it — for any target: web apps, mobile apps, desktop apps,
  plugin/extension UIs (Lightroom, VS Code, Figma), embedded displays, or any SDK/toolkit.
  Trigger on requests to "create an HTML mockup", "sketch the interface", "prototype the
  dialog/panel/form/dashboard", "show me how it would look", or any request to visualize a UI
  layout. Also trigger when the user wants to explore how a UI would work within specific
  platform constraints (SDK widget limitations, screen size limits, toolkit restrictions).
  Do NOT trigger for fixing CSS bugs in existing code, building production React/Vue/Angular
  components, accessibility audits of existing UIs, or adding features to already-implemented
  interfaces.
---

# UI Prototype — Rapid Interface Prototyping

Create interactive UI prototypes as single self-contained HTML files. The prototype
serves as a visual reference for how the interface should look and behave, regardless
of the target technology.

## Workflow

### Phase 1: Understand the Objective

Start by gathering essential context from the user:

1. **What is the UI for?** Get a clear description of the interface's purpose and audience.
2. **Target environment**: web app, mobile app, desktop app, plugin/extension for a
   specific platform, embedded UI, etc.
3. **Constraints**: ask about both technical and design constraints. Technical constraints
   come from the target platform (SDK limitations, available widgets, layout rules);
   design constraints come from the user (branding, accessibility, size limits, etc.).

If documentation about the target environment exists in the project, read it to extract
relevant constraints. The user may also provide constraints verbally — capture both sources.

For constrained environments (plugin SDKs, embedded UIs, specific toolkits), constraints
are the most valuable part of the prototyping process. Invest time in understanding the
target platform deeply: what widgets exist, what layout is possible, what events are
available, what workarounds are commonly used. A prototype that looks great but ignores
platform limitations gives a false sense of what's achievable.

### Phase 2: Establish Constraints

Save all identified constraints to a `constraints.md` file alongside the prototype HTML.
This file is the single source of truth for what the UI can and cannot do.

Structure of `constraints.md`:

```
# UI Prototype Constraints

## Target Environment
[Environment name and brief description]

## Technical Constraints
- [Constraint from SDK/platform docs or user input]
- [...]

## Design Constraints
- [User-defined constraint: branding, sizes, etc.]
- [...]

## Acknowledged Violations
- [Constraints the user chose to ignore, with reason]
```

Present the constraints to the user for confirmation before starting the prototype.
The user can add, modify, or remove constraints at any time during the process.

### Phase 3: Build the Prototype

Generate a **single self-contained HTML file** with all CSS and JavaScript inline.

**Prototype standards:**
- Use clean, semantic HTML structure
- CSS should approximate the look and feel of the target environment when possible
  (e.g., native-looking buttons for desktop, Material-style for Android)
- Include interactive behavior where useful (button clicks, tab switching, form inputs)
- Add a subtle annotation bar at the top showing the prototype name and target environment
- Use placeholder content that feels realistic (not "Lorem ipsum" — use contextually
  appropriate sample data)
- For desktop and plugin UIs, include a theme toggle (light/dark) using a small button
  in the annotation bar. Store the preference in `localStorage` so it persists across
  page reloads. Define both themes as CSS custom properties for easy switching.

**After generating the file**, ask the user:
> "Do you want me to open the prototype in your browser, or just save the file?"

Act according to their preference. If they want it opened, use the appropriate command
for the platform.

### Phase 4: Iterate

The user will request changes: adding elements, modifying layout, changing behavior,
removing components. For each request:

1. **Check against constraints**: does the request conflict with any established constraint?
2. **If no conflict**: apply the change directly.
3. **If conflict detected**:
   - Clearly explain which constraint is violated and why.
   - Suggest an alternative that achieves a similar goal within the constraints.
   - Let the user decide: they can accept the alternative, or override the constraint.
   - If overridden, add the violation to the "Acknowledged Violations" section of
     `constraints.md` with the user's reasoning.

After applying changes, offer to reopen the file in the browser if they had it open before.

### Constraint Violation Notices

When the prototype contains acknowledged violations, display them as a visible
notice panel at the bottom of the HTML interface. This panel should:

- Have a distinct visual style (e.g., warning-colored border) so it stands out
  from the prototype content
- List each violated constraint with a brief explanation
- Be clearly labeled as "Constraint Violations" so reviewers understand its purpose

This makes violations immediately visible to anyone viewing the prototype, not just
those reading the source code.

Also, when a constraint conflict was detected and resolved with an alternative
(e.g., splitting a form into multiple steps to respect a field-per-screen limit),
display a brief "Constraint Resolution" note near the relevant UI area explaining
what was adapted and why. This helps reviewers understand that the design choice
was intentional, not arbitrary.

## Guidelines

- **Prototype != production code.** Favor speed and clarity over code elegance. Inline
  styles and scripts are fine — this is a throwaway artifact for visualization.
- **Be opinionated about defaults.** When the user doesn't specify details (colors,
  spacing, fonts), pick sensible defaults that match the target environment rather
  than asking for every detail.
- **Show, don't describe.** When suggesting alternatives for constraint violations,
  modify the prototype to show the alternative rather than just describing it in text.
- **Keep constraints alive.** Re-read `constraints.md` before applying changes —
  constraints may have been updated during the conversation.
