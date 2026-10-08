# Previous-task recovery verification

7 October 2026. This checkpoint verifies the MCP route/back stack after background process termination on emulator-5554, using both the dedicated debug Activity and the actual MainActivity with isolated storage. It does not establish full-session, pending-permission, physical-device or all-route recovery.

## What the workflow proves

The debug fixture has a separate task affinity and owned persistent Room, DataStore and vault storage under `cache/task-recovery-fixture`. It uses the production PortalNavigation/PortalViewModel/repository classes. Its host points to closed loopback port 1, so no agent session can start. The shell entry point requires Android's DUMP permission and an app-owned enabled marker; the Activity is absent from the generated release manifest.

Two separate JUnit storage phases surround an external adb UI workflow. Preparation creates only fixture data and closes its database/scope. The external workflow launches the fixture, opens Settings → MCP chooser → host servers → editor and enters an unsaved name. It backgrounds the task, verifies completed framework state saving and matching PID/task metadata, checks that the app is no longer foreground, terminates only that verified process and verifies process absence.

Without starting instrumentation in between, the workflow reopens the same launcher intent. It asserts a distinct PID, the same task ID and non-null saved framework state. Android restores the MCP host route, while the editor draft remains absent. Back returns to the MCP connection chooser and then the Settings page, identified by its actual visible controls. The fixture task is explicitly removed afterward. Final JUnit verification confirms the saved definition and alias, unchanged production MCP aliases and removal of owned storage.

The final full workflow passed with task 1009. Historical task IDs/PIDs are evidence of that run only. Both storage phases passed individually. Existing 25 app JVM tests, debug/test APK builds, lint and release manifest generation pass. This increment changed debug/test infrastructure and its script; production navigation required no correction.

## Run it

Build with JDK 21 and install both current APKs first. Do not start another instrumentation while the external kill/restore workflow is running.

```powershell
# Repository root, after building/installing both debug APKs
./scripts/verify-task-recovery.ps1 -AdbPath 'C:/Users/<you>/AppData/Local/Android/Sdk/platform-tools/adb.exe'
# Actual MainActivity with an owned debug repository
./scripts/verify-task-recovery.ps1 -AdbPath 'C:/Users/<you>/AppData/Local/Android/Sdk/platform-tools/adb.exe' -MainActivity
```

MainActivity mode uses a debug-only TaskRecoveryFixtureApplication. Its normal startup constructs the production repository; only the app-owned `main-enabled` fixture marker switches presentation to the fixture store. Activity lifecycle callbacks record MainActivity task identity and completed state saving. Cleanup unregisters callbacks, closes the fixture store and restores the normal repository. Production PortalApplication is open with a protected repository setter to support this debug subclass. Release manifests keep PortalApplication and contain neither fixture Application nor fixture Activity.

The MainActivity script creates the marker and stops the app before the initial launch so a new Application reads it. At the end it stops the fixture app before storage verification. It does not clear app data. Hash assertions cover production host records, sessions and durable UI preferences; production MCP aliases and the owned saved definition/alias are also checked. Fingerprints avoid copying full user conversations into the checkpoint file.

If MainActivity mode fails, the marker keeps the debug app in its owned fixture mode for diagnosis. To return to normal debug storage while preserving evidence, remove only `cache/task-recovery-fixture/main-enabled` under `run-as dev.acportal`, then stop/restart the app. Do not delete production databases/preferences or wipe app/AVD data.

The script accepts `-Serial` for the selected emulator/device. Revalidate identity, settings and any existing fixture/task before running. It preserves production definitions and makes no device size/font/rotation changes. It removes its own `/sdcard/acportal-task-recovery.xml` in finally. A failed run leaves fixture-owned storage for diagnosis. Do not wipe app data or rerun preparation against that storage. `-Prepared` skips storage preparation only when the already prepared fixture is still at the initial page; inspect its current state before using it.

Force-stop alone can retain MainActivity in Recents. The corrected final MainActivity workflow passed with task 1019. Final verification removes only the task ID recorded by the owned fixture, checks its component belongs to the allowed app Activities and asserts its absence. An interrupted older cleanup can use `TaskRecoveryNavigationTest#removeRecordedFixtureTaskAfterInterruptedCleanup` with `-e fixtureTaskId` set to a task ID verified from that fixture's records and current Recents. Do not use it to clear unrelated tasks.

Do not run all methods of `TaskRecoveryNavigationTest` as a single suite. Invoke preparation, run the external workflow and then invoke verification, as the script does. Starting fresh instrumentation during restoration replaced the task in the initial harness, so that approach failed the same-task assertion. An initial XML check also assumed a selected-tab flag that Android did not expose. The final workflow checks the restored page's visible Settings controls instead.

## Remaining lifecycle gates

- More MainActivity routes, window configurations and unavailable-host/storage states.
- Restored live/offline/interrupted conversations, drafts, scroll and expanded activity state.
- Pending permissions reconciled against the host before enabling decisions.
- Background/lock/network transitions on a physical device with trusted TLS.
- MCP failures or a missing connection during task restoration.

MCP secrets remain excluded from saved instance state. Restoring an editor draft after process death would need an explicit secure persistence design; this test expects the unsaved draft to be discarded while saved definitions remain available.

MainActivity recovery checkpoint: the final external workflow passed on actual MainActivity using an owned debug repository. It confirms an unsaved editor value remains present after keyboard dismissal, then saves/backgrounds the task, verifies identity, terminates only its process and restores the same task ID in a distinct process. The MCP host route and Back through chooser/Settings return; the unsaved draft does not. Two standalone JUnit storage phases pass and verify saved fixture data/alias, unchanged production MCP aliases and hashes of production hosts, sessions and UI preferences. Cleanup removes the recorded fixture task from Recents, unregisters debug lifecycle callbacks, closes the owned store and restores the normal repository. Three normal debug regressions pass without skips: one McpLifecycleNavigationTest and two AgentDetailsLayoutUiTest. Final 25 app JVM tests, both APK builds, lint and release manifest generation pass. Release retains PortalApplication and excludes both debug fixture classes. The only production code change opens PortalApplication with a protected setter for debug subclass isolation. Core/Rust sources were unchanged and not rerun. More routes, full-session/pending-permission, physical-device and release acceptance remain open.
