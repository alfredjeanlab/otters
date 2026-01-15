# macOS Integration

Native macOS notifications provide real-time feedback on runbook events.

## Notifications

Events emit desktop notifications so developers stay informed without watching terminals:

| Event | Notification |
|-------|--------------|
| `pipeline:complete` | "auth build finished" |
| `pipeline:failed` | "auth build failed" (with alert sound) |
| `worker:idle` | "bugfix worker idle" |
| `escalate` | "Needs attention" (with alert sound) |

### osascript

One integration approach uses `osascript` to display native notifications:

```bash
# Basic notification
osascript -e 'display notification "Build complete" with title "Armor"'
# With subtitle
osascript -e 'display notification "3 issues resolved" with title "Armor" subtitle "bugfix worker"'
```

## Automation

AppleScript can interact with other macOS apps:

```bash
# Open URL in browser
osascript -e 'open location "https://github.com/org/repo/pull/123"'
# Bring Terminal to front
osascript -e 'tell application "Terminal" to activate'
```
