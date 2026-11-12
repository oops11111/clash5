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

## Existing application components

| Reference | Shared implementation |
| --- | --- |
| Confirmation dialog | `BaseDialog`, including its busy/disabled confirmation action |
| Error toast | `showNotice.error` and `NoticeManager`, preserving error code and detail behavior |
| Empty state | `BaseEmpty` with page-specific, localized explanation and available action |
| Loading skeleton | MUI `Skeleton` for initial content loads; preserve actual loading state |
| Button loading | MUI button `loading` state and disabled state while the operation is pending |
| Keyboard focus | Global `:focus-visible` outline from the custom theme; do not remove keyboard focus styling |

New R14 pages should reuse these shared components rather than rendering mock values or defining conflicting one-off state treatments.
