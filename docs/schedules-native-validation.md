# Native schedule recovery verification

Partial ticket21 evidence, 2026-11-09; neither ticket21 nor22 is accepted yet. Native product baseline `c79dc5d`.

## Behavior

Five focused native integration cases pass in `/tmp/schedules-native-overdue.log`. They cover foreground-only scheduling, background exclusion, lost acknowledgement and restart with immutable command identity, switch guards, legacy state decoding, restore epoch/cancellation, and an overdue refusal that preserves expense delivery. The HTTP batch independently verifies678 overdue entries as400+278, stable receipts and first-transfer creation. This document adds rendered control verification, not another network qualification.

## Rendered controls

`ExpenseWorkflowTest.scheduleRecoveryKeepsExpenseCaptureAndExplicitRequestControls` uses the production `ExpensesScreen` and theme in a Compose test activity, with selected Spending account and a pending schedule recovery fixture. Every run asserts the Add expense action remains visible, then scrolls to and activates both Retry scheduled entries and Cancel saved schedule request. Each run passed on an API36 emulator:

| Capture | Configuration | Evidence |
| --- | --- | --- |
| [Phone](screenshots/schedules-native/phone.png) |1080×2400,420dpi,light,font scale1.0|`/tmp/schedules21-native-phone.log`|
| [Enlarged dark phone](screenshots/schedules-native/dark-large.png) |1080×2400,420dpi,dark,font scale2.0|`/tmp/schedules21-native-dark-large.log`|
| [Tablet](screenshots/schedules-native/tablet.png) |1600×2560,240dpi,light,font scale1.0|`/tmp/schedules21-native-tablet.log`|

Root's Impeccable native audit found readable recovery text, reachable controls, preserved expense capture and no material clipping or overlap. The tablet retains the existing bounded content column. Large-font content scrolls. Independent review is pending.

The opt-in instrumentation argument `scheduleCapture=true` emits `SCHEDULE_CAPTURE_READY` after Compose settles and holds the fixture for10 seconds. The host waits one further second for composition before `adb exec-out screencap -p`; ordinary tests incur no capture delay. APK and instrumentation compilation pass in `/tmp/schedules21-native-render-build.log` and `/tmp/schedules21-native-capture-build.log`.

Initial captures were invalid: an emulator System UI unresponsive dialog obscured the app, followed by captures before the compositor painted. Those attempts remain under `.scratch/schedules21-native/` as `*-systemui-blocked.*` and `phone-late-capture.*`; none are accepted screenshots. Dismissing the emulator dialog and delaying capture produced the final images above. No production UI change was needed.

## Limits and cleanup

These are production composable renders in a test `ComponentActivity`, not network workflows or `MainActivity` system-bar verification. Light status-bar icon contrast belongs to that test host. Network/replay behavior is covered separately above. No physical-device, TalkBack, performance or full API-level compatibility claim is made. The owned emulator's geometry, font and night settings were reset, then the emulator stopped. No user device, live budget, production volume, signing configuration or release was changed.
