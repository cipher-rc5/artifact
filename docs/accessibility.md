# Accessibility Checklist

Production releases should validate:

- Keyboard navigation reaches dashboard, browser, results, history, settings, confirmation dialogs, and cleanup actions.
- Focus order follows visual order.
- Destructive actions have clear labels and confirmation text.
- Text and critical UI states meet contrast expectations in the default theme.
- Important state changes are represented in text, not color alone.
- The app remains usable at the documented minimum window size.
- Motion is limited to subtle progress/status affordances.

Current status: GPUI-level automated accessibility coverage is limited, so this checklist should be run manually before public release and after major view changes.
