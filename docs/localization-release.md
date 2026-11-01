# WindowTabs v0.2.0

This is the first published release of WindowTabs.

## Initial capabilities

- Group eligible already-open Windows application windows behind a separate native tab strip, with member switching, reordering and detaching.
- Save group definitions for explicit restoration against matching open windows and preserve original placements in a recovery journal.
- Restore applications when groups close rather than closing the applications. Elevated/protected/custom-window compatibility remains limited and requires the documented interactive validation.

## Localization and appearance

### Added

- Switch the interface live between English, Russian, French, German, Spanish, and Italian from Settings. Native language names make the selector recognizable in every language.
- Persist a Light or Dark theme and apply it to the native manager, lists, fields, buttons, and tab strips. Windows high-contrast colors remain authoritative.
- Translate group actions, recovery guidance, selection and empty states, saved-group metadata, tab-strip menus, and update check/download/install status messages. Window titles, group names, application paths, and technical diagnostic arguments retain their original contents.
- Read existing settings without losing saved groups or the startup-update opt-out; new preferences default to English and Light.

### Validation and limits

- Automated checks cover 148 catalog messages and preserve every placeholder in all five translated catalogs. Parameterized status tests preserve versions and diagnostic data; storage tests cover old settings, saved groups and preference round trips. Native GDI font measurements confirm that translated action captions fit the minimum-size layout.
- Native GDI rendering and the blocking message loop remain in use. Changing a preference invalidates existing visible controls once; hidden windows retain dirty state. No recurring redraw timer was added.
- GUI execution remains restricted for this pass. Translated-label clipping, interactive switching, actual dark tab-strip appearance, idle/hidden performance, and refreshed main/Settings screenshots have not been verified. Existing screenshots are labeled 0.1.0.
- Portable Windows x64 asset: **WindowTabs-v0.2.0.exe**, with **SHA256SUMS.txt**. Settings and recovery data remain in LocalAppData. Live grouping and end-to-end update installation remain pending.
