# 2026-04-23 Terminal Cursor, Mouse, and Scrollbar Session

## Session Goal

Prioritize terminal core comfort issues:

- Fix internal terminal cursor handling with Chinese / wide characters.
- Let mouse clicks choose the insertion position in normal shell input where possible.
- Add a terminal-side scrollbar so scrollback position is visible.

## Implementation Notes

- Keep PTY mouse passthrough semantics for apps that explicitly enable mouse mode.
- For normal shells that do not enable mouse mode, map same-line left clicks to left/right cursor movement escape sequences.
- Preserve the existing AI-agent-friendly input philosophy: no new global shortcuts.

## Changes Made

- Fixed wide-character cursor math in terminal rendering by treating `ScreenContent.cursor` as an already visual terminal column instead of recomputing it from cell display widths.
- Rendered the software cursor directly into the ratatui buffer, including both cells of a wide CJK glyph when the cursor overlays one.
- Normalized vt100 wide-character continuation cells to `display_width = 0`, matching the alacritty spacer semantics used by the renderer.
- Added scrollback metadata (`scroll_offset`, `scrollback_len`) to `ScreenContent` and `ScreenDiff`, and propagated it through daemon push updates.
- Drew a slim right-side terminal scrollbar when alacritty scrollback exists.
- Added shell-friendly mouse insertion: when the inner app has not enabled mouse mode, left-clicking the current cursor row sends left/right cursor movement escape sequences to place the insertion point.

## Verification

- `cargo test --no-run` passed.
- Focused regression tests passed:
  - `cargo test wide_char -- --nocapture`
  - `cargo test cursor_move_sequence -- --nocapture`
  - `cargo test wide_cursor_marks -- --nocapture`
  - `cargo test scroll_metadata_propagates_through_diff -- --nocapture`
  - `cargo test screen_diff_bell_and_focus_events_both_true -- --nocapture`
- Full `cargo test` currently fails in pre-existing `tests/input_tests.rs` expectations around the newer new-desk popup flow and rename Delete handling; those files were already changed outside this terminal cursor work.

## Follow-up: Global Alarm for Finished Agent Terminals

### Goal

Add a global attention layer for managing many Codex/Claude Code style terminals. Activity spinners show "working now"; alarms show "this terminal produced meaningful output and then went quiet, so it likely needs the next instruction."

### Detection Heuristic

- Reuse daemon `GetIdleStatus` instead of adding PTY polling.
- A tab is considered active when daemon idle seconds are `< 2`.
- The client records when each tab first enters active output.
- If a tab was active for at least 8 seconds, then becomes quiet, it enters a candidate state.
- If it remains quiet for 6 more seconds, it becomes an alarm.
- Any new output clears the alarm/candidate.
- Viewing the active tab acknowledges and clears its alarm.

This intentionally detects "meaningful output burst ended" rather than "process exited", because agent CLIs often keep the shell process alive while waiting for the next prompt.

### UI

- Topbar: alarmed tabs show a soft blinking `⚑` marker.
- Sidebar: each Desk shows `⚑N` where `N` is the number of alarmed terminals in that Desk.
- Existing activity spinners stay unchanged and take priority while a terminal is actively producing output.

### Files Changed

- `src/client/status.rs`
- `src/client/app.rs`
- `src/client/ui/topbar.rs`
- `src/client/ui/sidebar.rs`
- `src/main.rs`

### Verification

- `cargo test alarm_ -- --nocapture` passed.
- `cargo test viewing_active_tab_acknowledges_alarm -- --nocapture` passed.
- `cargo test --no-run` passed.
- Full `cargo test` still fails on the same pre-existing `tests/input_tests.rs` cases:
  - `n_in_sidebar_creates_task`
  - `rename_mode_cursor_and_delete_work_in_middle`

## Follow-up: Input Returns Scrollback to Bottom

### Goal

Fix the case where a terminal remains scrolled into history after keyboard input or paste, making newly typed input appear visually suspended above the bottom of the terminal.

### Changes Made

- Updated `PtyProvider::write()` to reset the emulator scrollback display offset to bottom before writing input bytes.
- Updated `PtyProvider::paste()` to reset to bottom before bracketed-paste detection and paste payload writing.
- Updated `DaemonProvider::write()` and `DaemonProvider::paste()` to send a scroll-to-bottom request before real input/paste, covering the normal client-to-daemon path.
- Updated the daemon subscribe fast path to handle `ClientMsg::Scroll`, so scroll-to-bottom and input can travel through the same low-latency worker connection.
- Included `scroll_offset` and `scrollback_len` in daemon `ScreenDiff` metadata-change detection so scroll-only viewport changes are pushed to the client.

### Verification

- `cargo test content_ --test input_tests -- --nocapture` passed.
- `cargo test alacritty_ -- --nocapture` passed.
- `cargo test scroll_ -- --nocapture` passed.
- `cargo install --path /home/kali/mato_corn/mato --force` completed successfully.
- Synced `/home/kali/.local/bin/mato` from `/home/kali/.cargo/bin/mato` because the active `PATH` resolves `mato` from `.local/bin` while `cargo install` writes to `.cargo/bin`.
- `cargo fmt` could not run in this environment because the `cargo-fmt` subcommand is not installed.

### Runtime Note

Existing daemon processes keep running old code until restarted. `cargo install` replaces the binary on disk, but does not restart the already-running daemon or its PTYs.

## Follow-up: Alarm Mode Toggle

### Goal

Add an explicit switch for the global alarm layer so users can turn attention alarms off while keeping normal activity spinners.

### Changes Made

- Added `App::alarm_enabled`, defaulting to `true`.
- Persisted the alarm switch in `SavedState` as `alarm_enabled`; old state files default to enabled.
- Added `App::toggle_alarm_mode()` and `App::clear_alarm_state()`.
- Settings now shows `Alarm mode on/off`, and `a` toggles it.
- When alarm mode is disabled:
  - existing alarm tabs/candidates/active-duration tracking are cleared;
  - new activity snapshots still update active spinners;
  - alarm candidates are not generated.

### Verification

- `cargo build` passed.
- `cargo test alarm_ -- --nocapture` passed.
- `cargo test settings_a_toggles_alarm_mode_and_clears_alarm_state -- --nocapture` passed.
- Full `cargo test` still fails on the same pre-existing `tests/input_tests.rs` cases:
  - `n_in_sidebar_creates_task`
  - `rename_mode_cursor_and_delete_work_in_middle`
