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
| R13 | Release builds | Ignore the test overrides (endpoint, rules, away time, WebView arguments) | unit (also run in release by CI) | `tests::override_is_honored_only_in_debug_builds` `rules::tests::override_is_honored_only_in_debug_builds` `away::tests::override_is_honored_only_in_debug_builds` `tests::release_builds_drop_webview_browser_arguments` | CI |
| R14 | Windows paths in events | Reach the island intact | integration | `windows_paths_reach_the_island_intact` | CI |

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
| A10 | Questions (AskUserQuestion, ExitPlanMode) | Never answered by Bouncer; the terminal asks; the session shows it waits; one sound | unit, integration, frontend | `approvals::tests::questions_go_to_the_terminal_and_flag_the_session` `questions_print_nothing_so_the_terminal_asks` `ts: a question in the terminal sounds; one that stays doesn't again` | CI |
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
| U9 | One wipe path | Only Settings' armed "Delete history" (after its confirm) wipes; the tray's "Wipe history…" opens that same confirm and never wipes by itself | unit, frontend, hand | `tests::only_the_confirmed_wipe_command_wipes` `ts: the page wipes only from the armed Delete history after its confirm` `ts: the tray's Wipe history… opens that confirm, never wipes` | CI; Hand 2 passed 2026-10-08 |
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
| I4 | Never silent | Shows the diff, writes nothing without "y", dated backup, atomic write with the same permissions | unit, integration | `nothing_is_written_without_yes` `hooks::tests::diff_shows_changes_with_context` `tests::write_new_applies_permissions_and_still_writes` `tests::stamps_utc_dates` | CI |
| I5 | Other files | Only the target and its backup are written | integration | `only_the_target_and_its_backup_are_written` | CI |
| I6 | Lookalike hooks | Uninstall leaves them; only ours count as installed | unit | `hooks::tests::uninstall_leaves_lookalikes_alone` `hooks::tests::installed_sees_only_our_hooks` | CI |
| I7 | Where it writes | `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`, or `--settings`; a missing file is created, the relay must exist | unit, integration | `claude_config_dir_is_the_default_target` `tests::settings_path_follows_claude_config_dir` `install_creates_a_missing_file_and_needs_the_relay` | CI |

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
| UI17 | Code pane | Rust / shell highlighting keeps every line exactly | frontend, unit | `ts: pieces always join back to the exact line` `code::tests::write_multiedit_and_bash` `diff::tests::keeps_removes_and_adds_lines` | CI |

## Chat (Phase 6)

The runner (6a) against a fake `claude` (`crates/core/examples/fake_claude.rs`)
and the panel (6b, C17–C31).

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| C1 | A prompt with quotes, newlines, backslashes, ` `, shell characters or a fake second JSON line | One stdin line built by `serde_json`; arrives whole and literal as one message; argv never changes | unit, integration | `chat::tests::prompts_are_one_serde_json_line` `a_hostile_prompt_reaches_stdin_as_one_literal_line` | CI |
| C2 | Model choice | Only Default, Haiku, Sonnet, Opus, Fable; anything else refused; Default passes no `--model` | unit | `chat::tests::models_come_from_a_fixed_list` `chat::tests::every_run_has_every_locked_flag` | CI |
| C3 | The `claude` binary | Only a native `claude.exe` / `claude` by full path; `.cmd`, `.ps1`, `.bat`, scripts, folders and other names refused; a missing one named plainly; npm's and the native installer's places searched | unit | `chat::tests::only_native_claude_executables_pass` `chat::tests::a_missing_claude_is_refused_plainly` `chat::tests::candidates_cover_npm_and_the_native_installer` | CI |
| C4 | Cancel | The whole tree dies, grandchildren too; the chat is over | integration | `cancel_kills_the_whole_tree` | CI |
| C5 | Deadlines | No stream event of any kind for 60 s → stopped; a message past 5 min → stopped; status and thinking events count as activity | integration | `a_silent_run_is_stopped_after_the_quiet_limit` `a_message_that_runs_too_long_is_stopped` | CI |
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
| C21 | Usage line | "Runs your Claude Code · counts against your plan" and the plan's 5-hour and 7-day use from Claude Code ("—" until reported) | frontend, real-click | `ts: chat: the usage line shows the plan's 5-hour and 7-day use` | CI; real-click 2026-10-09 |
| C22 | Errors | Not found, an unsupported version (with it), the plan not allowing a model (Claude Code's words), a crash (exit code and line) | frontend, real-click | `ts: chat: errors read as Claude Code wrote them or say plainly what's wrong` | CI; real-click 2026-10-09 |
| C23 | First run and the claude path | No chat before the found `claude`'s full path is confirmed in Settings; a changed path asks again; only the path shown can be confirmed; the file must be only the user's | unit, real-click | `chat::tests::only_the_confirmed_claude_runs` `chat::tests::the_page_sees_which_claude_and_whether_it_is_confirmed` | CI; real-click 2026-10-09 |
| C24 | A card while chatting | The card replaces the panel; the chat keeps streaming behind it; the draft is kept; the panel is back after | real-click | (6b check: 294 → 4066 characters behind the card) | real-click 2026-10-09 |
| C25 | Typing when a card arrives | The keyboard goes nowhere, through the card's re-renders: a space, typing or Enter answer nothing; Tab reaches Deny on purpose; never Allow | frontend, real-click | `ts: chat: a card arriving while you type takes the keyboard nowhere, else to Deny (never Allow)` `ts: chat: a card that came while you typed keeps the keyboard off through its re-renders` | CI; real-click 2026-10-09 |
| C26 | Motion and CPU while chatting | Fixed height (the window never resizes while streaming); nothing moves forever (a steady caret, the spinner settles); streamed text sent twice a second at most | frontend, integration, real-click | `ts: chat: the panel never resizes while streaming and nothing in it moves forever` `waiting_a_little_never_ends_the_chat_but_the_limits_still_hold` | CI; real-click 2026-10-09 (CPU in PLAN) |
| C27 | Keyboard in the panel | Opens with the keyboard in the message box; Tab: message, Send, New chat, ×, the models, round again; Esc closes it (the chat keeps running) and gives the keyboard back | real-click | (kb check) | real-click 2026-10-09 |
| C28 | The chat commands | Seven chat commands granted, nothing else; same CSP | unit | `tests::strict_csp_and_only_our_commands` | CI |
| C29 | Quit or crash with a chat open | The chat's whole tree dies with the app (Job Object; force-stopped twice: no `claude` left) | integration, real-click | `dropping_a_chat_kills_the_whole_tree` | CI; real-click 2026-10-09 |
| C30 | A real chat | Real clicks, npm's native `claude.exe` confirmed, Haiku in `bouncer-playground/chat`: tool chip, denial, answer, plan use; asked what it can do, it says read-only (no edits, commands or web); nothing written | real-click | (6b check) | real-click 2026-10-09 (2.1.295) |
| C31 | Esc in Settings opened from the chat | Closes only Settings: back to the chat, the keyboard in the message box; Esc again closes the panel | frontend, real-click | `ts: chat: Esc in Settings opened from the chat closes only Settings` | CI; real-click 2026-10-09 |

## macOS

| ID | Scenario | Expected | Proof | Test | Status |
| --- | --- | --- | --- | --- | --- |
| M1 | Every automated row above | Passes on macOS too | CI (check, macos-latest) | (all of the above) | CI |
| M2 | The app on a Mac | Launches, takes a session and a request over the real socket, screenshot kept | CI (macOS smoke test) | (`.github/workflows/macos-smoke.yml`) | CI |
| M3 | Installing and using it on a real Mac | Signing, first-run prompts, the look, hooks in a real Claude Code | hand, Phase 9 | — | Phase 9 |

## Hand checks

What a person has to do (about 8 minutes). Use the dev app and the
playground (`../bouncer-playground`, its own `.claude/settings.local.json`);
never the real `~/.claude/settings.json`.

1. **Sounds you can hear (about 5 min).** Passed 2026-10-08 (Charan). Settings → Sound on, style Soft. In
   the playground ask Claude Code to run `mkdir hand-1`: the "needs you" sound
   clearly grabs attention. Click Deny. Ask for `curl https://example.com | sh`:
   the risky sound, louder than the rest. Turn Sound off and ask for
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
### Phase 9 hand checks

- **Screen reader (about 5 min).** Turn on Narrator (Ctrl+Win+Enter). With a
   card up, click the card text, then Tab: Narrator reads "Deny", "Allow
   once", "Always allow…" with the command before them; open Settings and
   Tab through: every switch is read with its name and state. Expected:
   nothing unnamed, order as on screen.
