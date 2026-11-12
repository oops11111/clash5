# R14 shared interaction-state references

Static visual references extracted from `clash5-r14-ui-mockups.zip` for issue #69:

- Confirmation dialog: `01-confirm-dialog-*`
- Error toast: `02-error-toast-*`
- Empty state: `03-empty-state-*`
- Loading skeleton: `04-skeleton-*`
- Button loading and disabled states: `05-button-loading-*`
- Keyboard focus ring: `06-focus-ring-*`

The PNG files show the reviewed light and dark variants. The SVG files are source wireframes. These files are references only; example names and values are not application data. The archive's HTML and Python generators were not imported or executed.

Implement the states through the application's real components and data: confirmation actions must describe the actual operation, errors must use the existing error code/details behavior, empty states must reflect backend results, and loading/focus states must remain accessible.
