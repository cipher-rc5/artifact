//! GPUI view layer for ARTIFACT.
//!
//! `ArtifactView` owns the window-level chrome (sidebar, topbar, notice banner,
//! footer, delete-confirmation modal) and routes to one of the screen modules
//! (`dashboard`, `results`, `browser`, `history`, `settings`). Shared visual
//! primitives and pure helpers live in `widgets`.

mod browser;
mod dashboard;
mod history;
mod results;
mod settings;
mod widgets;

use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crate::app::{ArtifactApp, HistoryRun, NoticeKind, ScanState, StatusNotice};
use artifact::config::DeleteMode;
use artifact::directory_item::DirectoryType;
use artifact::rules;
use artifact::theme::{DesignSystem, Gradients};
use artifact::utils::{self, format_number};

use widgets::{
    alpha, sidebar_icon_name, summarize_artifacts, summarize_languages, summary_windows,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarView {
    Dashboard,
    Results,
    Browser,
    History,
    Settings,
}

#[derive(Clone, Copy)]
enum SidebarIcon {
    Dashboard,
    Results,
    Browser,
    History,
    Settings,
}

#[derive(Clone)]
struct ViewEntry {
    path: String,
    path_buf: PathBuf,
    dir_type: DirectoryType,
    project_name: String,
    size_bytes: u64,
    selected: bool,
    is_orphaned: bool,
}

#[derive(Clone)]
struct ArtifactBucket {
    label: String,
    size_bytes: u64,
}

#[derive(Clone)]
struct LanguageSetting {
    label: &'static str,
    enabled: bool,
    enabled_count: usize,
    total_count: usize,
    color: Hsla,
}

pub struct ArtifactView {
    app: Entity<ArtifactApp>,
    design: DesignSystem,
    active_view: SidebarView,
    // Row/run expansion is keyed by the item's stable path / run id, not a
    // vector index, so it survives the mid-delete `retain(..)` (review M4).
    expanded_rows: HashSet<PathBuf>,
    expanded_runs: HashSet<i64>,
    inventory_scroll: ScrollHandle,
    activity_scroll: ScrollHandle,
    browser_scroll: ScrollHandle,
    history_scroll: ScrollHandle,
    languages_scroll: ScrollHandle,
    history_cache: Vec<HistoryRun>,
    history_error: Option<String>,
    system_id: String,
}

impl ArtifactView {
    pub fn new(app: Entity<ArtifactApp>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_view, _entity, cx| cx.notify()).detach();

        // Adaptive progress-drain loop (review finding H4). Rather than a
        // forever 200ms spin, the loop:
        //   * exits when the app entity is gone (`cx.update` returns Err) — one
        //     loop per view is torn down instead of leaking immortally;
        //   * polls fast (120ms) only while there is active work
        //     (scanning / deleting / browsing / history loading);
        //   * otherwise idles at a slow 1s tick, which is enough to expire a
        //     stale notice without burning CPU or defeating power management.
        let app_clone = app.clone();
        cx.spawn(async move |view, cx: &mut AsyncApp| {
            loop {
                // Stop the loop once the view is torn down, instead of leaking an
                // immortal poller per view.
                if view.upgrade().is_none() {
                    break;
                }

                // Drain worker progress and decide whether there is active work.
                // `cx.update` / `app.update` return `Err` if the entity is gone,
                // which is also our exit signal.
                let active = app_clone.update(cx, |app, cx| {
                    app.check_scan_progress(cx);
                    app.check_delete_progress(cx);
                    app.check_browse_progress(cx);
                    app.expire_notice_if_stale(cx);

                    app.scan_state() == ScanState::Scanning
                        || app.is_deleting()
                        || app.is_browse_loading()
                        || app.is_history_loading()
                });

                let active = match active {
                    Ok(active) => active,
                    Err(_) => break,
                };

                // Move any completed async history load into the view cache.
                if view
                    .update(cx, |view, cx| view.drain_history_progress(cx))
                    .is_err()
                {
                    break;
                }

                // Fast tick while work is in flight; slow idle tick otherwise so
                // notice expiry still fires without spinning the CPU.
                let interval = if active {
                    Duration::from_millis(120)
                } else {
                    Duration::from_millis(1000)
                };
                cx.background_executor().timer(interval).await;
            }
        })
        .detach();

        Self {
            app,
            design: DesignSystem::new(),
            active_view: SidebarView::Dashboard,
            expanded_rows: HashSet::new(),
            expanded_runs: HashSet::new(),
            inventory_scroll: ScrollHandle::new(),
            activity_scroll: ScrollHandle::new(),
            browser_scroll: ScrollHandle::new(),
            history_scroll: ScrollHandle::new(),
            languages_scroll: ScrollHandle::new(),
            history_cache: Vec::new(),
            history_error: None,
            system_id: hostname::get()
                .ok()
                .and_then(|n| n.into_string().ok())
                .unwrap_or_else(|| "WORKSTATION".to_string())
                .to_uppercase(),
        }
    }

    fn navigate_to_view(&mut self, view: SidebarView, cx: &mut Context<Self>) {
        self.active_view = view;
        self.app.update(cx, |app, cx| {
            if app.is_file_browser_open() {
                app.close_file_browser(cx);
            }
        });
        if matches!(view, SidebarView::History) {
            self.refresh_history(cx);
        }
        cx.notify();
    }

    /// Kick off an async history load off the UI thread (review findings
    /// H4/H5/H6). Results are drained by `drain_history_progress`.
    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        self.history_error = None;
        self.app
            .update(cx, |app, cx| app.start_history_load(500, cx));
        // Pick up an immediately-ready result (e.g. the no-DB empty case).
        self.drain_history_progress(cx);
    }

    /// Move a completed async history load, if any, into the view cache.
    fn drain_history_progress(&mut self, cx: &mut Context<Self>) {
        let result = self
            .app
            .update(cx, |app, cx| app.check_history_progress(cx));
        if let Some(result) = result {
            match result {
                Ok(runs) => {
                    self.history_cache = runs;
                    self.history_error = None;
                }
                Err(e) => {
                    self.history_cache = Vec::new();
                    self.history_error = Some(e);
                }
            }
            cx.notify();
        }
    }

    fn toggle_row_expanded(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.expanded_rows.insert(path.clone()) {
            self.expanded_rows.remove(&path);
        }
        cx.notify();
    }

    fn toggle_run_expanded(&mut self, run_id: i64, cx: &mut Context<Self>) {
        if !self.expanded_runs.insert(run_id) {
            self.expanded_runs.remove(&run_id);
        }
        cx.notify();
    }

    fn open_browser_view(&mut self, cx: &mut Context<Self>) {
        self.active_view = SidebarView::Browser;
        self.app.update(cx, |app, cx| {
            if !app.is_file_browser_open() {
                app.open_file_browser(cx);
            }
        });
        cx.notify();
    }
}

impl Render for ArtifactView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let d = self.design;
        let viewport_width = window.bounds().size.width;

        let scan_state = app.scan_state();
        let scan_path = app.scan_path().to_string();
        let total_size = app.total_size();
        let selected_size = app.selected_size();
        let deleted_count = app.deleted_count();
        let error_msg = app.error_message().map(|s| s.to_string());
        let delete_errors = app.delete_errors().to_vec();
        let can_cancel_delete = app.can_cancel_delete();
        let notice = app.notice().cloned();
        let delete_mode = app.delete_mode();
        let pending_delete = app.pending_delete();
        let is_deleting = app.is_deleting();
        let is_browse_loading = app.is_browse_loading();
        let enabled_rule_names: Vec<(&'static str, bool)> = rules::RULES
            .iter()
            .map(|r| (r.name, app.is_rule_enabled(r.name)))
            .collect();
        let language_settings = summarize_languages(d, &enabled_rule_names);
        let enabled_language_count = language_settings
            .iter()
            .filter(|setting| setting.enabled)
            .count();
        let show_orphaned = app.show_orphaned_only();
        let progress = app.scan_progress_data().cloned();
        let file_browser_open = app.is_file_browser_open();
        let browse_path = app.browse_path().display().to_string();
        let browse_entries: Vec<_> = app
            .browse_entries()
            .iter()
            .map(|e| (e.name.clone(), e.path.clone()))
            .collect();
        let can_browse_back = app.can_browse_back();
        let can_browse_forward = app.can_browse_forward();
        let scan_log: Vec<String> = app.scan_log().iter().rev().cloned().collect();

        let view_entries: Vec<ViewEntry> = app
            .visible_entries()
            .iter()
            .map(|(_, item)| ViewEntry {
                path: item.path.display().to_string(),
                path_buf: item.path.clone(),
                dir_type: item.dir_type,
                project_name: item.project_name.clone().unwrap_or_default(),
                size_bytes: item.size_bytes,
                selected: item.selected,
                is_orphaned: item.is_orphaned,
            })
            .collect();

        let active_view = if file_browser_open {
            SidebarView::Browser
        } else {
            self.active_view
        };

        let item_count = view_entries.len();
        let selected_count = view_entries.iter().filter(|entry| entry.selected).count();
        let selected_preview: Vec<String> = view_entries
            .iter()
            .filter(|entry| entry.selected)
            .take(5)
            .map(|entry| entry.path.clone())
            .collect();
        let artifact_buckets = summarize_artifacts(&view_entries);
        let chart_buckets = summary_windows(&artifact_buckets);
        let system_id = self.system_id.clone();
        let scan_dirs = app.directories_scanned().unwrap_or(0);
        let scan_elapsed = app.scan_elapsed_secs().unwrap_or(0.0);

        let _ = app;

        div()
            .size_full()
            .font_family("Menlo")
            .bg(d.colors.bg_primary)
            .text_color(d.colors.text_primary)
            .relative()
            .flex()
            .flex_row()
            .child(self.render_sidebar(d, active_view, scan_state, item_count > 0, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(Self::render_topbar(
                        d,
                        &system_id,
                        scan_state,
                        &scan_path,
                        scan_dirs,
                        scan_elapsed,
                        item_count,
                        viewport_width < px(1260.0),
                    ))
                    .when_some(notice.clone(), |root, notice| {
                        root.child(self.render_notice(
                            d,
                            &notice,
                            scan_state == ScanState::Complete
                                && active_view != SidebarView::Results,
                            cx,
                        ))
                    })
                    .child(
                        div()
                            .id("artifact-content")
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .px(px(14.0))
                            .pt(px(14.0))
                            .pb(px(10.0))
                            .child(match active_view {
                                SidebarView::Dashboard => self.render_dashboard_view(
                                    d,
                                    scan_state,
                                    progress.as_ref(),
                                    &artifact_buckets,
                                    &chart_buckets,
                                    &scan_log,
                                    total_size,
                                    item_count,
                                    selected_count,
                                    viewport_width,
                                    cx,
                                ),
                                SidebarView::Results => self.render_results_view(
                                    d,
                                    scan_state,
                                    &view_entries,
                                    total_size,
                                    selected_size,
                                    selected_count,
                                    error_msg.as_deref(),
                                    &delete_errors,
                                    deleted_count,
                                    delete_mode,
                                    is_deleting,
                                    can_cancel_delete,
                                    viewport_width,
                                    cx,
                                ),
                                SidebarView::Browser => self.render_browser_view(
                                    d,
                                    scan_state,
                                    &scan_path,
                                    &browse_path,
                                    &browse_entries,
                                    file_browser_open,
                                    can_browse_back,
                                    can_browse_forward,
                                    is_browse_loading,
                                    enabled_language_count,
                                    language_settings.len(),
                                    show_orphaned,
                                    viewport_width,
                                    cx,
                                ),
                                SidebarView::History => self.render_history_view(d, cx),
                                SidebarView::Settings => self.render_settings_view(
                                    d,
                                    &scan_path,
                                    &language_settings,
                                    delete_mode,
                                    viewport_width,
                                    cx,
                                ),
                            }),
                    )
                    .child(Self::render_footer(d)),
            )
            .when(pending_delete, {
                let app_confirm = self.app.clone();
                let app_cancel = self.app.clone();
                let mode_label = match delete_mode {
                    DeleteMode::Trash => "Move To Trash",
                    DeleteMode::Permanent => "Delete Permanently",
                };
                let warning = match delete_mode {
                    DeleteMode::Trash => "Selected artifacts will be moved to Trash.",
                    DeleteMode::Permanent => {
                        "Selected artifacts will be permanently deleted. This cannot be undone."
                    }
                };
                let summary = format!(
                    "{} director{} — {}",
                    selected_count,
                    if selected_count == 1 { "y" } else { "ies" },
                    utils::format_size(selected_size)
                );
                let preview = selected_preview.clone();
                let confirm_label = match delete_mode {
                    DeleteMode::Trash => "Confirm",
                    DeleteMode::Permanent => "Permanently Delete",
                };
                move |this| {
                    let app_cancel2 = app_cancel.clone();
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .bg(gpui::rgba(0x00000099u32))
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                app_cancel.update(cx, |app, cx| app.cancel_delete_confirm(cx));
                            })
                            .child(
                                div()
                                    .bg(d.colors.bg_secondary)
                                    .border_1()
                                    .border_color(d.colors.border_primary)
                                    .p(px(28.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(18.0))
                                    .w(px(420.0))
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, _| {})
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(d.colors.text_primary)
                                            .child(mode_label),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(d.colors.text_secondary)
                                            .child(warning),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(d.colors.text_secondary)
                                            .child(summary),
                                    )
                                    .child(div().flex().flex_col().gap(px(4.0)).children(
                                        preview.iter().map(|path| {
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(d.colors.text_tertiary)
                                                .child(path.clone())
                                        }),
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(12.0))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .py(px(8.0))
                                                    .px(px(14.0))
                                                    .bg(d.colors.accent_green)
                                                    .text_color(d.colors.bg_primary)
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .cursor_pointer()
                                                    .on_mouse_down(
                                                        gpui::MouseButton::Left,
                                                        move |_, _, cx| {
                                                            app_confirm.update(cx, |app, cx| {
                                                                app.delete_selected(cx)
                                                            });
                                                        },
                                                    )
                                                    .child(confirm_label),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .py(px(8.0))
                                                    .px(px(14.0))
                                                    .border_1()
                                                    .border_color(d.colors.border_primary)
                                                    .text_color(d.colors.text_secondary)
                                                    .text_size(px(11.0))
                                                    .cursor_pointer()
                                                    .on_mouse_down(
                                                        gpui::MouseButton::Left,
                                                        move |_, _, cx| {
                                                            app_cancel2.update(cx, |app, cx| {
                                                                app.cancel_delete_confirm(cx)
                                                            });
                                                        },
                                                    )
                                                    .child("Cancel"),
                                            ),
                                    ),
                            ),
                    )
                }
            })
    }
}

// ---------------------------------------------------------------------------
// Window chrome: sidebar, topbar, notice, footer.
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_sidebar(
        &self,
        d: DesignSystem,
        active_view: SidebarView,
        scan_state: ScanState,
        has_results: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let live_color = match scan_state {
            ScanState::Idle => d.colors.text_tertiary,
            ScanState::Scanning => d.colors.accent_orange,
            ScanState::Complete => d.colors.accent_green,
        };

        div()
            .w(px(70.0))
            .h_full()
            .bg(Gradients::sidebar_surface(&d.colors))
            .border_r_1()
            .border_color(d.colors.border_primary)
            .flex()
            .flex_col()
            .items_center()
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .h(px(70.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::app_mark(d)),
            )
            .child(Self::separator(d))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.0))
                    .pt(px(16.0))
                    .child(Self::sidebar_button(
                        d,
                        SidebarIcon::Dashboard,
                        active_view == SidebarView::Dashboard,
                        cx.listener(|this, _, _, cx| {
                            this.navigate_to_view(SidebarView::Dashboard, cx);
                        }),
                    ))
                    .child(Self::sidebar_button(
                        d,
                        SidebarIcon::Results,
                        active_view == SidebarView::Results,
                        cx.listener(|this, _, _, cx| {
                            this.navigate_to_view(SidebarView::Results, cx);
                        }),
                    ))
                    .child(Self::sidebar_button(
                        d,
                        SidebarIcon::Browser,
                        active_view == SidebarView::Browser,
                        cx.listener(|this, _, _, cx| {
                            this.open_browser_view(cx);
                        }),
                    ))
                    .child(Self::sidebar_button(
                        d,
                        SidebarIcon::History,
                        active_view == SidebarView::History,
                        cx.listener(|this, _, _, cx| {
                            this.navigate_to_view(SidebarView::History, cx);
                        }),
                    ))
                    .child(Self::sidebar_button(
                        d,
                        SidebarIcon::Settings,
                        active_view == SidebarView::Settings,
                        cx.listener(|this, _, _, cx| {
                            this.navigate_to_view(SidebarView::Settings, cx);
                        }),
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(10.0))
                    .pb(px(18.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded_full()
                                    .bg(if has_results {
                                        live_color
                                    } else {
                                        d.colors.text_tertiary
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child(match scan_state {
                                        ScanState::Idle => "Idle",
                                        ScanState::Scanning => "Scan",
                                        ScanState::Complete => "Done",
                                    }),
                            ),
                    ),
            )
    }

    fn sidebar_button(
        d: DesignSystem,
        icon: SidebarIcon,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let mut button = div()
            .id(ElementId::Name(
                format!("side-{}", sidebar_icon_name(icon)).into(),
            ))
            .relative()
            .w(px(44.0))
            .h(px(44.0))
            .border_1()
            .border_color(if active {
                d.colors.accent_green
            } else {
                d.colors.border_primary
            })
            .rounded(d.radius.xs)
            .flex()
            .items_center()
            .justify_center()
            .hover(|style| style.bg(alpha(d.colors.text_primary, 0.06)))
            .active(|style| style.bg(alpha(d.colors.text_primary, 0.10)))
            .cursor_pointer()
            .on_click(move |event, window, app| on_click(event, window, app))
            .child(Self::render_sidebar_icon(d, icon, active));

        if active {
            button = button.bg(Gradients::cta_emphasized(&d.colors)).child(
                div()
                    .absolute()
                    .left(px(-1.0))
                    .top(px(4.0))
                    .bottom(px(4.0))
                    .w(px(2.0))
                    .bg(d.colors.accent_green),
            );
        }

        button
    }

    #[allow(clippy::too_many_arguments)]
    fn render_topbar(
        d: DesignSystem,
        system_id: &str,
        scan_state: ScanState,
        scan_path: &str,
        scan_dirs: usize,
        scan_elapsed: f64,
        artifact_count: usize,
        compact: bool,
    ) -> Div {
        let status_text = match scan_state {
            ScanState::Idle => "Idle",
            ScanState::Scanning => "Scan_Active",
            ScanState::Complete => "Scan_Complete",
        };

        let identity = div()
            .flex()
            .items_end()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::BLACK)
                    .text_color(d.colors.text_primary)
                    .child("ARTIFACT"),
            )
            .child(
                div()
                    .pb(px(2.0))
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_secondary)
                    .child(concat!("BUILD CLEANUP v", env!("CARGO_PKG_VERSION"))),
            );

        let session_line = match scan_state {
            ScanState::Idle if artifact_count == 0 => "Session: None".to_string(),
            ScanState::Scanning => format!(
                "Session: {} DIRS / {}",
                format_number(scan_dirs),
                utils::format_elapsed(scan_elapsed)
            ),
            _ => format!("Session: {} ARTIFACTS", format_number(artifact_count)),
        };

        let telemetry = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(18.0))
            .child(Self::topbar_block(
                d,
                &format!("System_id: {system_id}"),
                &format!("Status: {status_text}"),
            ))
            .child(Self::topbar_block(
                d,
                &format!("Path: {scan_path}"),
                &session_line,
            ));

        let accent_line = div()
            .h(px(1.0))
            .w_full()
            .bg(Gradients::header_strip(&d.colors));

        if compact {
            div()
                .w_full()
                .border_b_1()
                .border_color(d.colors.border_primary)
                .bg(Gradients::topbar_surface(&d.colors))
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(18.0))
                        .py(px(14.0))
                        .flex()
                        .flex_col()
                        .gap(px(14.0))
                        .child(identity)
                        .child(telemetry),
                )
                .child(accent_line)
        } else {
            div()
                .w_full()
                .border_b_1()
                .border_color(d.colors.border_primary)
                .bg(Gradients::topbar_surface(&d.colors))
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(18.0))
                        .py(px(14.0))
                        .flex()
                        .items_center()
                        .gap(px(18.0))
                        .child(identity)
                        .child(div().flex_1())
                        .child(telemetry),
                )
                .child(accent_line)
        }
    }

    fn topbar_block(d: DesignSystem, line_one: &str, line_two: &str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_secondary)
                    .child(line_one.to_string()),
            )
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(line_two.to_string()),
            )
    }

    fn render_notice(
        &mut self,
        d: DesignSystem,
        notice: &StatusNotice,
        show_results_action: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let accent = match notice.kind {
            NoticeKind::Success => d.colors.status_success,
            NoticeKind::Error => d.colors.status_error,
        };

        div()
            .mx(px(14.0))
            .mt(px(14.0))
            .relative()
            .p(px(14.0))
            .pl(px(18.0))
            .border_1()
            .border_color(alpha(accent, 0.55))
            .bg(Gradients::notice_glow(accent))
            .rounded(d.radius.xs)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(14.0))
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .bottom(px(0.0))
                    .w(px(3.0))
                    .bg(Gradients::accent_strip(accent)),
            )
            .child(div().w(px(8.0)).h(px(8.0)).bg(accent))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(d.typography.size_sm)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(d.colors.text_primary)
                            .child(notice.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_sm)
                            .text_color(d.colors.text_secondary)
                            .child(notice.message.clone()),
                    ),
            )
            .when(show_results_action, |banner| {
                banner.child(Self::terminal_button(
                    d,
                    "notice-results",
                    "Open Results",
                    true,
                    false,
                    cx.listener(|this, _, _, cx| {
                        this.navigate_to_view(SidebarView::Results, cx);
                    }),
                ))
            })
            .child(Self::notice_close_button(
                d,
                cx.listener(|this, _, _, cx| {
                    this.app.update(cx, |app, cx| app.dismiss_notice(cx));
                }),
            ))
    }

    fn notice_close_button(
        d: DesignSystem,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id("notice-close")
            .w(px(26.0))
            .h(px(26.0))
            .ml(px(4.0))
            .border_1()
            .border_color(d.colors.border_primary)
            .rounded(d.radius.xs)
            .bg(Gradients::cta_quiet(&d.colors))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.border_color(d.colors.text_primary))
            .on_click(move |event, window, app| on_click(event, window, app))
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .font_weight(FontWeight::BLACK)
                    .text_color(d.colors.text_secondary)
                    .child("X"),
            )
    }

    fn render_sidebar_icon(d: DesignSystem, icon: SidebarIcon, active: bool) -> Div {
        let color = if active {
            d.colors.text_primary
        } else {
            d.colors.text_secondary
        };

        match icon {
            SidebarIcon::Dashboard => div()
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .flex_wrap()
                .gap(px(2.0))
                .children((0..4).map(|_| {
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .border_1()
                        .border_color(color)
                        .bg(alpha(color, if active { 0.16 } else { 0.04 }))
                })),
            SidebarIcon::Results => div()
                .w(px(18.0))
                .h(px(18.0))
                .border_1()
                .border_color(color)
                .rounded(px(3.0))
                .flex()
                .flex_col()
                .justify_center()
                .px(px(3.0))
                .gap(px(2.0))
                .child(div().w_full().h(px(2.0)).bg(color))
                .child(div().w_full().h(px(2.0)).bg(color))
                .child(div().w(px(8.0)).h(px(2.0)).bg(color)),
            SidebarIcon::Browser => div()
                .w(px(18.0))
                .h(px(16.0))
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(1.0))
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(alpha(color, 0.9)),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(11.0))
                        .border_1()
                        .border_color(color)
                        .rounded(px(3.0))
                        .bg(alpha(color, if active { 0.12 } else { 0.03 })),
                ),
            SidebarIcon::History => div()
                .w(px(18.0))
                .h(px(18.0))
                .border_1()
                .border_color(color)
                .rounded_full()
                .relative()
                .child(
                    div()
                        .absolute()
                        .top(px(3.0))
                        .left(px(7.0))
                        .w(px(2.0))
                        .h(px(6.0))
                        .bg(color),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .left(px(8.0))
                        .w(px(5.0))
                        .h(px(2.0))
                        .bg(color),
                ),
            SidebarIcon::Settings => div()
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(3.0))
                .child(Self::slider_icon_row(color, px(1.0), px(6.0)))
                .child(Self::slider_icon_row(color, px(8.0), px(6.0)))
                .child(Self::slider_icon_row(color, px(4.0), px(6.0))),
        }
    }

    fn slider_icon_row(color: Hsla, knob_offset: Pixels, knob_width: Pixels) -> Div {
        div()
            .w(px(18.0))
            .h(px(3.0))
            .bg(alpha(color, 0.28))
            .rounded_full()
            .child(
                div()
                    .ml(knob_offset)
                    .w(knob_width)
                    .h(px(3.0))
                    .bg(color)
                    .rounded_full(),
            )
    }

    fn render_footer(d: DesignSystem) -> Div {
        div()
            .h(px(36.0))
            .px(px(18.0))
            .border_t_1()
            .border_color(d.colors.border_secondary)
            .flex()
            .items_center()
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child("© 2026 ARTIFACT"),
            )
    }
}
