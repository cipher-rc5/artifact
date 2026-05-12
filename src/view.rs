use gpui::prelude::FluentBuilder;
use gpui::*;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::app::{ArtifactApp, HistoryRun, NoticeKind, ScanState, StatusNotice};
use artifact::config::DeleteMode;
use artifact::directory_item::DirectoryType;
use artifact::rules::{self, ColorHint};
use artifact::theme::{DesignSystem, Gradients};
use artifact::utils;

fn rule_color(d: DesignSystem, hint: ColorHint) -> Hsla {
    match hint {
        ColorHint::Green => d.colors.accent_green,
        ColorHint::Orange => d.colors.accent_orange,
        ColorHint::Blue => d.colors.accent_blue,
        ColorHint::Yellow => d.colors.accent_yellow,
        ColorHint::Purple => d.colors.accent_purple,
        ColorHint::Red => d.colors.accent_red,
    }
}

fn alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

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
    index: usize,
    path: String,
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
    expanded_rows: HashSet<usize>,
    expanded_runs: HashSet<i64>,
    inventory_scroll: ScrollHandle,
    activity_scroll: ScrollHandle,
    browser_scroll: ScrollHandle,
    history_scroll: ScrollHandle,
    languages_scroll: ScrollHandle,
    history_cache: Vec<HistoryRun>,
    history_loaded_at: Option<std::time::Instant>,
    history_error: Option<String>,
    system_id: String,
}

impl ArtifactView {
    pub fn new(app: Entity<ArtifactApp>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_view, _entity, cx| cx.notify()).detach();

        let app_clone = app.clone();
        cx.spawn(async move |_this, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let _ = cx.update(|cx| {
                    app_clone.update(cx, |app, cx| {
                        app.check_scan_progress(cx);
                        app.check_delete_progress(cx);
                        app.expire_notice_if_stale(cx);
                    });
                });
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
            history_loaded_at: None,
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

    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        match self.app.read(cx).load_history(500) {
            Ok(runs) => {
                self.history_cache = runs;
                self.history_error = None;
            }
            Err(e) => {
                self.history_cache = Vec::new();
                self.history_error = Some(e);
            }
        }
        self.history_loaded_at = Some(std::time::Instant::now());
    }

    fn toggle_row_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.expanded_rows.insert(index) {
            self.expanded_rows.remove(&index);
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
        let notice = app.notice().cloned();
        let delete_mode = app.delete_mode();
        let pending_delete = app.pending_delete();
        let is_deleting = app.is_deleting();
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
            .map(|(i, item)| ViewEntry {
                index: *i,
                path: item.path.display().to_string(),
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
                    .when(notice.is_some(), |root| {
                        root.child(self.render_notice(
                            d,
                            notice.as_ref().unwrap(),
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
                                    deleted_count,
                                    delete_mode,
                                    is_deleting,
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
                    .child("BUILD CLEANUP V2.4.0"),
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

    fn app_mark(d: DesignSystem) -> Div {
        div()
            .w(px(28.0))
            .h(px(28.0))
            .rounded(d.radius.xs)
            .border_1()
            .border_color(d.colors.accent_green)
            .bg(Gradients::cta_emphasized(&d.colors))
            .flex()
            .items_center()
            .justify_center()
            .child(div().w(px(8.0)).h(px(8.0)).bg(d.colors.accent_green))
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

    #[allow(clippy::too_many_arguments)]
    fn render_dashboard_view(
        &mut self,
        d: DesignSystem,
        scan_state: ScanState,
        progress: Option<&crate::app::ScanProgress>,
        artifact_buckets: &[ArtifactBucket],
        chart_buckets: &[ArtifactBucket],
        scan_log: &[String],
        total_size: u64,
        item_count: usize,
        selected_count: usize,
        viewport_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = viewport_width < px(1180.0);
        let dense = viewport_width < px(1380.0);
        let side_panel_width = if dense { px(280.0) } else { px(340.0) };
        let bucket_segment_width = if dense { px(26.0) } else { px(34.0) };
        let progress_dirs = progress.map_or(0, |p| p.directories_scanned);
        let progress_items = progress.map_or(item_count, |p| p.items_found.max(item_count));
        let progress_size = progress.map_or(total_size, |p| p.total_size_found.max(total_size));
        let progress_elapsed = progress.map_or(0.0, |p| p.elapsed_secs);
        let progress_path = progress.map(|p| p.current_path.clone()).unwrap_or_default();
        let status_label = match scan_state {
            ScanState::Idle => "System_Ready",
            ScanState::Scanning => "Scan_Active",
            ScanState::Complete => "Scan_Complete",
        };
        let readiness = match scan_state {
            ScanState::Idle => 0,
            ScanState::Scanning => progress
                .and_then(|p| {
                    let total = p.total_dirs?;
                    if total == 0 {
                        return None;
                    }
                    Some(
                        ((p.directories_scanned as f64 / total as f64) * 99.0).clamp(1.0, 99.0)
                            as usize,
                    )
                })
                .unwrap_or(1),
            ScanState::Complete => 100,
        };
        let center_button_label = match scan_state {
            ScanState::Idle => "Initiate Scan",
            ScanState::Scanning => "Scanning",
            ScanState::Complete => "Results",
        };
        let button_enabled = scan_state != ScanState::Scanning;
        let app_scan = self.app.clone();
        let app_rescan = self.app.clone();
        let app_reset = self.app.clone();

        let left_column = div()
            .w(side_panel_width)
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(Self::panel(
                d,
                "Build_Artifacts_Found",
                "H15 // Archive",
                div()
                    .flex_1()
                    .min_h_0()
                    .px(px(16.0))
                    .py(px(14.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .overflow_hidden()
                    .children(if artifact_buckets.is_empty() {
                        vec![
                            div()
                                .text_size(d.typography.size_sm)
                                .text_color(d.colors.text_tertiary)
                                .child("No Artifact Clusters Detected"),
                        ]
                    } else {
                        artifact_buckets
                            .iter()
                            .take(4)
                            .map(|bucket| {
                                let filled =
                                    scaled_segments(bucket.size_bytes, artifact_buckets, 7);
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_size(d.typography.size_sm)
                                                    .text_color(d.colors.text_primary)
                                                    .child(bucket.label.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(d.typography.size_sm)
                                                    .text_color(d.colors.text_secondary)
                                                    .child(utils::format_size(bucket.size_bytes)),
                                            ),
                                    )
                                    .child(Self::meter_bar(
                                        d,
                                        filled,
                                        7,
                                        d.colors.text_primary,
                                        bucket_segment_width,
                                        px(10.0),
                                    ))
                            })
                            .collect()
                    }),
            ))
            .child(Self::panel(
                d,
                "Savings_Analysis",
                "H16 // Disk",
                div()
                    .flex_1()
                    .min_h_0()
                    .px(px(16.0))
                    .py(px(14.0))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .gap(px(12.0))
                    .overflow_hidden()
                    .child(Self::render_savings_chart(d, chart_buckets))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(26.0))
                                    .font_weight(FontWeight::BLACK)
                                    .text_color(d.colors.text_primary)
                                    .child(utils::format_size(progress_size)),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_secondary)
                                    .child("Total Recoverable Space"),
                            ),
                    ),
            ));

        let center_column =
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .child(div().w_full().flex_shrink_0().flex().justify_end().child(
                    if scan_state == ScanState::Complete {
                        div().child(Self::terminal_button(
                            d,
                            "dashboard-reset",
                            "Reset",
                            true,
                            false,
                            cx.listener(move |_, _, _, cx| {
                                app_reset.update(cx, |app, cx| app.reset_scan(cx));
                            }),
                        ))
                    } else {
                        div()
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(14.0))
                        .child(Self::render_gauge(
                            d,
                            readiness,
                            status_label,
                            item_count,
                            progress_dirs,
                            progress_elapsed,
                            &progress_path,
                            dense,
                            matches!(scan_state, ScanState::Scanning),
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .justify_center()
                                .gap(px(24.0))
                                .child(Self::status_callout(
                                    d,
                                    "Status",
                                    status_label,
                                    match scan_state {
                                        ScanState::Idle => d.colors.text_secondary,
                                        ScanState::Scanning => d.colors.accent_orange,
                                        ScanState::Complete => d.colors.accent_green,
                                    },
                                ))
                                .child(Self::status_callout(
                                    d,
                                    "Artifacts",
                                    &format!("{} Found", format_number(progress_items)),
                                    d.colors.text_primary,
                                )),
                        )
                        .child(if scan_state == ScanState::Complete {
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap(px(8.0))
                                .child(Self::terminal_button(
                                    d,
                                    "dashboard-cta",
                                    center_button_label,
                                    true,
                                    true,
                                    cx.listener(move |this, _, _, cx| {
                                        this.navigate_to_view(SidebarView::Results, cx);
                                    }),
                                ))
                                .child(Self::terminal_button(
                                    d,
                                    "dashboard-rescan",
                                    "Rescan",
                                    true,
                                    false,
                                    cx.listener(move |_, _, _, cx| {
                                        app_rescan.update(cx, |app, cx| app.start_scan(cx));
                                    }),
                                ))
                        } else {
                            div().flex().items_center().justify_center().child(
                                Self::terminal_button(
                                    d,
                                    "dashboard-cta",
                                    center_button_label,
                                    button_enabled,
                                    true,
                                    cx.listener(move |this, _, _, cx| match scan_state {
                                        ScanState::Idle => {
                                            app_scan.update(cx, |app, cx| app.start_scan(cx));
                                        }
                                        ScanState::Scanning => {}
                                        ScanState::Complete => {
                                            this.navigate_to_view(SidebarView::Results, cx);
                                        }
                                    }),
                                ),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .justify_center()
                                .items_center()
                                .gap(px(36.0))
                                .child(
                                    div()
                                        .text_size(d.typography.size_xs)
                                        .text_color(d.colors.text_tertiary)
                                        .child("CMD: artifact.exe --full-scan"),
                                )
                                .child(
                                    div()
                                        .text_size(d.typography.size_xs)
                                        .text_color(d.colors.text_tertiary)
                                        .child("REF: [H9-H10]"),
                                ),
                        ),
                );

        let selection_pct = if item_count == 0 {
            0
        } else {
            (selected_count * 100) / item_count.max(1)
        };
        let recoverable_total = utils::format_size(total_size);
        let selected_segments = if item_count == 0 {
            0
        } else {
            (selection_pct.max(1)).div_ceil(15).clamp(1, 7)
        };
        let right_column = div()
            .w(side_panel_width)
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(Self::panel(
                d,
                "Session_Metrics",
                "",
                div()
                    .flex_shrink_0()
                    .px(px(16.0))
                    .py(px(14.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(Self::health_metric(
                        d,
                        "Artifacts",
                        &format_number(item_count),
                        d.colors.accent_green,
                        item_count.div_ceil(50).clamp(0, 7),
                    ))
                    .child(Self::health_metric(
                        d,
                        "selected",
                        &format!("{selected_count} / {selection_pct}%"),
                        d.colors.accent_yellow,
                        selected_segments,
                    ))
                    .child(Self::health_metric(
                        d,
                        "total_size",
                        &recoverable_total,
                        d.colors.accent_blue,
                        if total_size == 0 { 0 } else { 4 },
                    )),
            ))
            .child(Self::panel(
                d,
                "Activity_Log",
                "live",
                self.render_activity_log(d, scan_log),
            ));

        if compact {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(px(12.0))
                .child(center_column)
                .child(left_column)
                .child(right_column)
        } else {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .gap(px(12.0))
                .child(left_column)
                .child(center_column)
                .child(right_column)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_results_view(
        &self,
        d: DesignSystem,
        scan_state: ScanState,
        entries: &[ViewEntry],
        total_size: u64,
        selected_size: u64,
        selected_count: usize,
        error_msg: Option<&str>,
        deleted_count: usize,
        delete_mode: DeleteMode,
        is_deleting: bool,
        viewport_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = viewport_width < px(1180.0);
        let app = self.app.clone();
        let max_bytes = entries
            .iter()
            .map(|entry| entry.size_bytes)
            .max()
            .unwrap_or(1);
        let scan_state_text = match scan_state {
            ScanState::Idle => "Idle",
            ScanState::Scanning => "Scanning",
            ScanState::Complete => "Ready",
        };

        let inventory_panel = Self::panel(
            d,
            "Artifact_Inventory",
            &format!("{} ITEMS", format_number(entries.len())),
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .px(px(14.0))
                .pb(px(12.0))
                .pt(px(8.0))
                .gap(px(8.0))
                .child(Self::inventory_header(d, compact))
                .child(self.render_inventory_rows(d, entries, max_bytes, compact, cx)),
        );

        let summary_panel = Self::panel(
            d,
            "Purge_Parameter_v2",
            "Action",
            Self::results_sidebar(
                d,
                total_size,
                selected_size,
                entries.len(),
                selected_count,
                scan_state_text,
                error_msg,
                deleted_count,
                delete_mode,
                is_deleting,
                app,
            ),
        );

        if compact {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(px(12.0))
                .child(inventory_panel)
                .child(summary_panel)
        } else {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .gap(px(12.0))
                .child(inventory_panel)
                .child(div().w(px(420.0)).flex_shrink_0().child(summary_panel))
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_browser_view(
        &mut self,
        d: DesignSystem,
        scan_state: ScanState,
        scan_path: &str,
        browse_path: &str,
        browse_entries: &[(String, PathBuf)],
        file_browser_open: bool,
        can_browse_back: bool,
        can_browse_forward: bool,
        enabled_language_count: usize,
        total_languages: usize,
        show_orphaned: bool,
        viewport_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = viewport_width < px(1100.0);
        let app_orphan = self.app.clone();
        let app_scan = self.app.clone();

        let browser_panel = Self::panel(
            d,
            "Select_Scan_Root",
            "Browser // FS",
            if file_browser_open {
                self.render_browser_list(
                    d,
                    browse_path,
                    browse_entries,
                    can_browse_back,
                    can_browse_forward,
                    cx,
                )
            } else {
                div()
                    .px(px(18.0))
                    .pb(px(18.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(d.colors.text_primary)
                            .child("Directory Browser Offline"),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_sm)
                            .text_color(d.colors.text_secondary)
                            .child("Open The File Browser To Change THE SCAN ROOT."),
                    )
                    .child(Self::terminal_button(
                        d,
                        "browser-open",
                        "Open Browser",
                        true,
                        false,
                        cx.listener(|this, _, _, cx| {
                            this.open_browser_view(cx);
                        }),
                    ))
            },
        );

        let control_panel = Self::panel(
            d,
            "Scan_Parameters",
            "P-V2 // Control",
            div()
                .px(px(18.0))
                .pb(px(18.0))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(Self::results_metric_line(
                    d,
                    "Scan_Root",
                    &truncate_end(scan_path, if compact { 28 } else { 32 }),
                ))
                .child(Self::results_metric_line(
                    d,
                    "Browse_Path",
                    &truncate_end(browse_path, if compact { 28 } else { 32 }),
                ))
                .child(Self::results_metric_line(
                    d,
                    "Languages_Enabled",
                    &format!("{} / {}", enabled_language_count, total_languages),
                ))
                .child(Self::results_metric_line(
                    d,
                    "Scan_State",
                    match scan_state {
                        ScanState::Idle => "Idle",
                        ScanState::Scanning => "Scanning",
                        ScanState::Complete => "Complete",
                    },
                ))
                .child(Self::separator(d))
                .child(Self::toggle_row(
                    d,
                    "Orphaned_Only",
                    show_orphaned,
                    move |_, _, cx| {
                        app_orphan.update(cx, |app, cx| app.toggle_orphaned_only(cx));
                    },
                ))
                .child(Self::separator(d))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(12.0))
                        .child(Self::terminal_button(
                            d,
                            "browser-settings",
                            "Settings",
                            true,
                            false,
                            cx.listener(|this, _, _, cx| {
                                this.navigate_to_view(SidebarView::Settings, cx);
                            }),
                        ))
                        .child(Self::terminal_button(
                            d,
                            "browser-scan",
                            "Run Scan",
                            scan_state != ScanState::Scanning,
                            true,
                            move |_, _, cx| {
                                app_scan.update(cx, |app, cx| app.start_scan(cx));
                            },
                        ))
                        .child(Self::terminal_button(
                            d,
                            "browser-return",
                            "Return",
                            true,
                            false,
                            cx.listener(|this, _, _, cx| {
                                this.navigate_to_view(SidebarView::Dashboard, cx);
                            }),
                        )),
                ),
        );

        if compact {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(px(14.0))
                .child(browser_panel)
                .child(control_panel)
        } else {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .gap(px(14.0))
                .child(browser_panel)
                .child(div().w(px(520.0)).flex_shrink_0().child(control_panel))
        }
    }

    fn render_browser_list(
        &self,
        d: DesignSystem,
        browse_path: &str,
        entries: &[(String, PathBuf)],
        can_browse_back: bool,
        can_browse_forward: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let app_cancel = self.app.clone();
        let app_select = self.app.clone();
        let app_back = self.app.clone();
        let app_forward = self.app.clone();

        let mut list = div()
            .id("browser-list")
            .track_scroll(&self.browser_scroll)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pl(px(2.0))
            .pr(px(12.0))
            .gap(px(4.0));

        if entries.is_empty() {
            list = list.child(
                div()
                    .py(px(12.0))
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_tertiary)
                    .child("No Subdirectories Available"),
            );
        } else {
            for (name, path) in entries {
                let app_nav = self.app.clone();
                let nav_path = path.clone();
                let is_parent = name == "..";
                let label = if is_parent {
                    "../".to_string()
                } else {
                    format!("{name}/")
                };

                list = list.child(
                    div()
                        .id(ElementId::Name(format!("browse-{}", path.display()).into()))
                        .px(px(12.0))
                        .py(px(8.0))
                        .border_1()
                        .border_color(d.colors.border_secondary)
                        .rounded(d.radius.xs)
                        .bg(Gradients::cta_quiet(&d.colors))
                        .cursor_pointer()
                        .hover(|style| {
                            style
                                .bg(Gradients::cta_emphasized(&d.colors))
                                .border_color(d.colors.accent_green)
                        })
                        .on_click(move |_, _, cx| {
                            app_nav.update(cx, |app, cx| app.browse_navigate(nav_path.clone(), cx));
                        })
                        .child(
                            div()
                                .text_size(d.typography.size_sm)
                                .text_color(if is_parent {
                                    d.colors.text_secondary
                                } else {
                                    d.colors.text_primary
                                })
                                .child(label),
                        ),
                );
            }
        }

        let list_with_overlay = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(list)
            .child(Self::scroll_overlay(d, &self.browser_scroll));

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(14.0))
            .pb(px(14.0))
            .pt(px(8.0))
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(Self::terminal_button_sm(
                        d,
                        "browse-back",
                        "<",
                        can_browse_back,
                        move |_, _, cx| {
                            app_back.update(cx, |app, cx| app.browse_back(cx));
                        },
                    ))
                    .child(Self::terminal_button_sm(
                        d,
                        "browse-forward",
                        ">",
                        can_browse_forward,
                        move |_, _, cx| {
                            app_forward.update(cx, |app, cx| app.browse_forward(cx));
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .overflow_hidden()
                            .child(truncate_end(browse_path, 60)),
                    ),
            )
            .child(Self::separator(d))
            .child(list_with_overlay)
            .child(Self::separator(d))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(Self::terminal_button(
                        d,
                        "browse-cancel",
                        "Cancel",
                        true,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            app_cancel.update(cx, |app, cx| app.close_file_browser(cx));
                            this.navigate_to_view(SidebarView::Dashboard, cx);
                        }),
                    ))
                    .child(Self::terminal_button(
                        d,
                        "browse-select",
                        "Select",
                        true,
                        true,
                        cx.listener(move |this, _, _, cx| {
                            app_select.update(cx, |app, cx| app.browse_select(cx));
                            this.navigate_to_view(SidebarView::Dashboard, cx);
                        }),
                    )),
            )
    }

    fn render_settings_view(
        &mut self,
        d: DesignSystem,
        scan_path: &str,
        language_settings: &[LanguageSetting],
        delete_mode: DeleteMode,
        viewport_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = viewport_width < px(1100.0);

        let languages_panel = Self::panel(
            d,
            "Scan_Languages",
            "Filters // Ruleset",
            self.language_settings_list(d, language_settings),
        );

        let actions_panel = Self::panel(
            d,
            "Delete_Behavior",
            "Safety // Action",
            div()
                .px(px(18.0))
                .pb(px(18.0))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(Self::results_metric_line(
                    d,
                    "Scan_Root",
                    &truncate_end(scan_path, if compact { 28 } else { 34 }),
                ))
                .child(Self::separator(d))
                .child(Self::delete_mode_option(
                    d,
                    DeleteMode::Trash,
                    delete_mode == DeleteMode::Trash,
                    "Move To Trash",
                    "Safer default. Files stay recoverable from the system trash.",
                    self.app.clone(),
                ))
                .child(Self::delete_mode_option(
                    d,
                    DeleteMode::Permanent,
                    delete_mode == DeleteMode::Permanent,
                    "Delete Permanently",
                    "Immediately removes artifacts from disk with no trash fallback.",
                    self.app.clone(),
                ))
                .child(Self::separator(d))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(12.0))
                        .child(Self::terminal_button(
                            d,
                            "settings-root",
                            "Change Scan Root",
                            true,
                            false,
                            cx.listener(|this, _, _, cx| {
                                this.open_browser_view(cx);
                            }),
                        ))
                        .child(Self::terminal_button(
                            d,
                            "settings-dashboard",
                            "Back To Dashboard",
                            true,
                            false,
                            cx.listener(|this, _, _, cx| {
                                this.navigate_to_view(SidebarView::Dashboard, cx);
                            }),
                        )),
                ),
        );

        if compact {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(px(14.0))
                .child(actions_panel)
                .child(languages_panel)
        } else {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .gap(px(14.0))
                .child(languages_panel)
                .child(div().w(px(460.0)).flex_shrink_0().child(actions_panel))
        }
    }

    fn render_history_view(&self, d: DesignSystem, cx: &mut Context<Self>) -> Div {
        let history_error = self.history_error.clone();
        let runs = self.history_cache.clone();
        let total_runs = runs.len();
        let total_records: usize = runs.iter().map(|r| r.entries.len()).sum();
        let total_bytes: i64 = runs.iter().map(|r| r.total_bytes).sum();

        let list_panel = Self::panel(
            d,
            "Cleanup_History",
            &format!("{} Runs", format_number(total_runs)),
            self.render_history_list(d, &runs, cx),
        );

        let summary_panel = Self::panel(
            d,
            "History_Summary",
            "Aggregate",
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .px(px(16.0))
                .py(px(14.0))
                .gap(px(14.0))
                .child(Self::results_metric_line(
                    d,
                    "Total_Runs",
                    &format_number(total_runs),
                ))
                .child(Self::separator(d))
                .child(Self::results_metric_line(
                    d,
                    "Total_Deletions",
                    &format_number(total_records),
                ))
                .child(Self::separator(d))
                .child(Self::results_metric_line(
                    d,
                    "Space_Reclaimed",
                    &utils::format_size(total_bytes.max(0) as u64),
                ))
                .child(Self::separator(d))
                .child(Self::terminal_button(
                    d,
                    "history-refresh",
                    "Refresh",
                    true,
                    false,
                    cx.listener(|this, _, _, cx| {
                        this.refresh_history(cx);
                        cx.notify();
                    }),
                )),
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(12.0))
            .when(history_error.is_some(), |this| {
                this.child(
                    div()
                        .px(px(14.0))
                        .py(px(8.0))
                        .text_size(d.typography.size_xs)
                        .text_color(d.colors.accent_red)
                        .child(format!(
                            "History Unavailable: {}",
                            history_error.unwrap_or_default()
                        )),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .gap(px(12.0))
                    .child(list_panel)
                    .child(div().w(px(280.0)).flex_shrink_0().child(summary_panel)),
            )
    }

    fn render_history_list(
        &self,
        d: DesignSystem,
        runs: &[HistoryRun],
        cx: &mut Context<Self>,
    ) -> Div {
        let mut list = div()
            .id("history-list")
            .track_scroll(&self.history_scroll)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pl(px(12.0))
            .pr(px(22.0))
            .pt(px(8.0))
            .pb(px(12.0))
            .gap(px(8.0));

        if runs.is_empty() {
            list = list.child(
                div()
                    .py(px(16.0))
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_tertiary)
                    .child("No Prior Cleanup Runs Recorded"),
            );
        } else {
            for run in runs {
                let run_id = run.started_at;
                let expanded = self.expanded_runs.contains(&run_id);
                let label = format_unix_time(run.started_at);
                let bytes = run.total_bytes.max(0) as u64;
                let toggle_id = run_id;

                let header = div()
                    .id(ElementId::Name(format!("history-run-{run_id}").into()))
                    .px(px(12.0))
                    .py(px(10.0))
                    .border_1()
                    .border_color(if expanded {
                        d.colors.accent_green
                    } else {
                        d.colors.border_secondary
                    })
                    .rounded(d.radius.xs)
                    .bg(if expanded {
                        Gradients::cta_emphasized(&d.colors)
                    } else {
                        Gradients::cta_quiet(&d.colors)
                    })
                    .cursor_pointer()
                    .hover(|style| style.bg(alpha(d.colors.text_primary, 0.05)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_run_expanded(toggle_id, cx);
                    }))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .w(px(10.0))
                                            .text_size(d.typography.size_xs)
                                            .text_color(d.colors.text_tertiary)
                                            .child(if expanded { "▾" } else { "▸" }.to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(d.typography.size_sm)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(d.colors.text_primary)
                                            .child(label),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_secondary)
                                    .child(format!(
                                        "{} Items // {}",
                                        format_number(run.entries.len()),
                                        utils::format_size(bytes)
                                    )),
                            ),
                    );

                let entry_block: Option<Div> = if expanded {
                    let mut block = div()
                        .pt(px(4.0))
                        .pl(px(20.0))
                        .pr(px(4.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0));
                    for entry in &run.entries {
                        block = block.child(
                            div()
                                .flex()
                                .items_start()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .w(px(64.0))
                                        .flex_shrink_0()
                                        .text_size(d.typography.size_xs)
                                        .text_color(d.colors.text_tertiary)
                                        .child(utils::format_size(entry.size_bytes.max(0) as u64)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(d.typography.size_xs)
                                        .text_color(d.colors.text_secondary)
                                        .child(entry.path.clone()),
                                ),
                        );
                    }
                    Some(block)
                } else {
                    None
                };

                let mut wrapper = div().flex().flex_col().gap(px(4.0)).child(header);
                if let Some(block) = entry_block {
                    wrapper = wrapper.child(block);
                }
                list = list.child(wrapper);
            }
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(list)
            .child(Self::scroll_overlay(d, &self.history_scroll))
    }

    fn language_settings_list(
        &self,
        d: DesignSystem,
        language_settings: &[LanguageSetting],
    ) -> Div {
        let app = &self.app;
        let mut list = div()
            .id("language-settings-list")
            .track_scroll(&self.languages_scroll)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pl(px(16.0))
            .pr(px(22.0))
            .pt(px(4.0))
            .pb(px(14.0));

        for (index, setting) in language_settings.iter().enumerate() {
            let app_language = app.clone();
            let language = setting.label;
            let next_enabled = !setting.enabled;

            if index > 0 {
                list = list.child(Self::separator(d));
            }

            list = list.child(
                div()
                    .py(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .w(px(8.0))
                                            .h(px(8.0))
                                            .rounded_full()
                                            .bg(setting.color),
                                    )
                                    .child(
                                        div()
                                            .text_size(d.typography.size_sm)
                                            .text_color(d.colors.text_primary)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(setting.label),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_secondary)
                                    .child(format!(
                                        "{} of {} artifact rules enabled",
                                        setting.enabled_count, setting.total_count
                                    )),
                            ),
                    )
                    .child(Self::action_toggle(
                        d,
                        ElementId::Name(format!("language-{language}").into()),
                        setting.enabled,
                        move |_, _, cx| {
                            app_language.update(cx, |app, cx| {
                                app.set_language_enabled(language, next_enabled, cx)
                            });
                        },
                    )),
            );
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(list)
            .child(Self::scroll_overlay(d, &self.languages_scroll))
    }

    fn delete_mode_option(
        d: DesignSystem,
        delete_mode: DeleteMode,
        active: bool,
        title: &'static str,
        description: &'static str,
        app: Entity<ArtifactApp>,
    ) -> Stateful<Div> {
        div()
            .id(ElementId::Name(
                format!("delete-mode-{:?}", delete_mode).into(),
            ))
            .p(px(14.0))
            .border_1()
            .border_color(if active {
                d.colors.accent_green
            } else {
                d.colors.border_primary
            })
            .bg(if active {
                Gradients::cta_emphasized(&d.colors)
            } else {
                Gradients::cta_quiet(&d.colors)
            })
            .rounded(d.radius.xs)
            .cursor_pointer()
            .hover(|style| style.bg(alpha(d.colors.text_primary, 0.06)))
            .on_click(move |_, _, cx| {
                app.update(cx, |app, cx| app.set_delete_mode(delete_mode, cx));
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(d.typography.size_sm)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(d.colors.text_primary)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .child(description),
                    ),
            )
    }

    fn scroll_overlay(d: DesignSystem, handle: &ScrollHandle) -> Div {
        let bounds = handle.bounds();
        let max = handle.max_offset();
        let visible: f32 = bounds.size.height.into();
        let max_height: f32 = max.height.into();
        let content = visible + max_height;

        let track_height = visible.max(1.0);
        let thumb_ratio = if content <= 0.0 || visible <= 0.0 {
            1.0_f32
        } else {
            (visible / content).clamp(0.08, 1.0)
        };
        let thumb_height = (track_height * thumb_ratio).max(24.0);

        let offset_y: f32 = handle.offset().y.into();
        let scroll_y = -offset_y;
        let max_y = max_height.max(0.0);
        let progress = if max_y <= 0.0 {
            0.0_f32
        } else {
            (scroll_y / max_y).clamp(0.0, 1.0)
        };
        let thumb_top = progress * (track_height - thumb_height).max(0.0);

        // Hide the bar entirely when there is no overflow.
        if max_y <= 0.5 {
            return div().w(px(0.0));
        }

        div()
            .absolute()
            .top(px(0.0))
            .right(px(4.0))
            .bottom(px(0.0))
            .w(px(8.0))
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(2.0))
                    .bottom(px(0.0))
                    .w(px(2.0))
                    .bg(alpha(d.colors.text_primary, 0.05)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(thumb_top))
                    .left(px(0.0))
                    .w(px(6.0))
                    .h(px(thumb_height))
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(alpha(d.colors.accent_green, 0.85), 0.0),
                        linear_color_stop(alpha(d.colors.accent_green, 0.35), 1.0),
                    ))
                    .border_l_1()
                    .border_color(alpha(d.colors.accent_green, 0.55)),
            )
    }

    fn panel(d: DesignSystem, title: &'static str, meta: &str, body: Div) -> Div {
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(Gradients::panel_surface(&d.colors))
            .border_1()
            .border_color(d.colors.border_primary)
            .rounded(d.radius.sm)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(2.0))
                    .w_full()
                    .bg(Gradients::header_strip(&d.colors)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(16.0))
                    .pt(px(12.0))
                    .pb(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .border_b_1()
                    .border_color(d.colors.border_secondary)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .pr(px(12.0))
                            .child(div().w(px(6.0)).h(px(6.0)).bg(d.colors.accent_green))
                            .child(Self::panel_label(d, title)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_tertiary)
                            .child(meta.to_string()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(body),
            )
    }

    fn panel_label(d: DesignSystem, text: &'static str) -> Div {
        div()
            .text_size(d.typography.size_sm)
            .text_color(d.colors.text_secondary)
            .font_weight(FontWeight::SEMIBOLD)
            .child(text)
    }

    fn separator(d: DesignSystem) -> Div {
        div().h(px(1.0)).w_full().bg(d.colors.border_secondary)
    }

    fn meter_bar(
        d: DesignSystem,
        filled: usize,
        total: usize,
        color: Hsla,
        segment_width: Pixels,
        segment_height: Pixels,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .children((0..total).map(|index| {
                if index < filled {
                    div().w(segment_width).h(segment_height).bg(linear_gradient(
                        90.0,
                        linear_color_stop(alpha(color, 0.95), 0.0),
                        linear_color_stop(alpha(color, 0.55), 1.0),
                    ))
                } else {
                    div()
                        .w(segment_width)
                        .h(segment_height)
                        .bg(alpha(d.colors.text_primary, 0.10))
                        .border_1()
                        .border_color(alpha(d.colors.text_primary, 0.06))
                }
            }))
    }

    fn render_savings_chart(d: DesignSystem, buckets: &[ArtifactBucket]) -> Div {
        let max = buckets
            .iter()
            .map(|bucket| bucket.size_bytes)
            .max()
            .unwrap_or(1);

        div()
            .w_full()
            .flex_1()
            .min_h(px(70.0))
            .max_h(px(110.0))
            .flex()
            .items_end()
            .gap(px(6.0))
            .children((0..4usize).map(|index| {
                let bucket = buckets
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| ArtifactBucket {
                        label: format!("W{}", index + 1),
                        size_bytes: 0,
                    });
                let height = if max == 0 {
                    20.0
                } else {
                    20.0 + (bucket.size_bytes as f32 / max as f32) * 58.0
                };
                let top = if index == 3 {
                    d.colors.accent_green
                } else {
                    alpha(d.colors.text_primary, 0.50 + (index as f32 * 0.10))
                };
                let bottom = alpha(top, 0.10);

                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .child(bucket.label),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(height))
                            .border_t_1()
                            .border_color(top)
                            .bg(linear_gradient(
                                180.0,
                                linear_color_stop(top, 0.0),
                                linear_color_stop(bottom, 1.0),
                            )),
                    )
            }))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_gauge(
        d: DesignSystem,
        readiness: usize,
        status_label: &str,
        item_count: usize,
        dirs_scanned: usize,
        elapsed_secs: f64,
        progress_path: &str,
        compact: bool,
        is_scanning: bool,
    ) -> Div {
        let outer_size = if compact { px(180.0) } else { px(220.0) };
        let inner_size = if compact { px(122.0) } else { px(150.0) };
        let readiness_size = if compact { px(28.0) } else { px(34.0) };

        // Pulse the ring opacity while a scan is active. The view re-renders
        // on every progress event (~50ms), giving a smooth 0.5 Hz breathe.
        let (outer_opacity, inner_opacity) = if is_scanning {
            let t = (elapsed_secs * std::f64::consts::PI * 0.5).sin() as f32 * 0.5 + 0.5;
            (0.20 + t * 0.20, 0.38 + t * 0.27)
        } else {
            (0.30, 0.55)
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .w(outer_size)
                    .h(outer_size)
                    .rounded_full()
                    .border_1()
                    .border_color(alpha(d.colors.accent_green, outer_opacity))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(inner_size)
                            .h(inner_size)
                            .rounded_full()
                            .border_2()
                            .border_color(alpha(d.colors.accent_green, inner_opacity))
                            .bg(Gradients::gauge_inner(&d.colors))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(d.typography.size_xs)
                                            .text_color(d.colors.text_secondary)
                                            .child(status_label.to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(readiness_size)
                                            .font_weight(FontWeight::BLACK)
                                            .text_color(d.colors.text_primary)
                                            .child(format!("{readiness}%")),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(format!(
                        "Sector 4F / BLOCK {}",
                        (item_count.max(12) % 89) + 10
                    )),
            )
            .when(dirs_scanned > 0, |el| {
                el.child(
                    div()
                        .text_size(d.typography.size_xs)
                        .text_color(d.colors.text_tertiary)
                        .child(format!(
                            "{} Dirs Tracked // {} // {}",
                            format_number(dirs_scanned),
                            utils::format_elapsed(elapsed_secs),
                            progress_path
                        )),
                )
            })
    }

    fn status_callout(d: DesignSystem, label: &str, value: &str, color: Hsla) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(px(22.0))
                    .font_weight(FontWeight::BLACK)
                    .text_color(color)
                    .child(value.to_string()),
            )
    }

    fn health_metric(d: DesignSystem, label: &str, value: &str, color: Hsla, filled: usize) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .child(label.to_string()),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_sm)
                            .text_color(color)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(value.to_string()),
                    ),
            )
            .child(Self::meter_bar(d, filled, 7, color, px(50.0), px(4.0)))
    }

    fn render_activity_log(&self, d: DesignSystem, scan_log: &[String]) -> Div {
        let mut log = div()
            .id("activity-log")
            .track_scroll(&self.activity_scroll)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pl(px(14.0))
            .pr(px(16.0))
            .pb(px(14.0))
            .pt(px(8.0))
            .gap(px(6.0));

        if scan_log.is_empty() {
            log = log.child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_tertiary)
                    .child("No Activity Recorded"),
            );
        } else {
            for (index, path) in scan_log.iter().enumerate() {
                log = log.child(
                    div()
                        .flex()
                        .gap(px(10.0))
                        .child(
                            div()
                                .w(px(40.0))
                                .flex_shrink_0()
                                .text_size(d.typography.size_xs)
                                .text_color(d.colors.text_tertiary)
                                .child(format!("#{:03}", index + 1)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(d.typography.size_xs)
                                .text_color(d.colors.text_secondary)
                                .child(path.clone()),
                        ),
                );
            }
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(log)
            .child(Self::scroll_overlay(d, &self.activity_scroll))
    }

    fn inventory_header(d: DesignSystem, compact: bool) -> Div {
        let header = |label: &str| {
            div()
                .text_size(d.typography.size_xs)
                .text_color(d.colors.text_tertiary)
                .child(label.to_string())
        };

        let base = div()
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(d.colors.border_secondary)
            .bg(Gradients::topbar_surface(&d.colors))
            .flex()
            .items_center()
            .gap(px(12.0));

        if compact {
            base.child(div().w(px(18.0)).flex_shrink_0())
                .child(header("Component_Path").flex_1())
                .child(header("Size").w(px(64.0)).flex_shrink_0())
                .child(header("Action").w(px(36.0)).flex_shrink_0())
        } else {
            base.child(div().w(px(18.0)).flex_shrink_0())
                .child(header("Component_Patj").flex_1())
                .child(header("Type").w(px(112.0)).flex_shrink_0())
                .child(header("Size").w(px(72.0)).flex_shrink_0())
                .child(header("Metric").w(px(96.0)).flex_shrink_0())
                .child(header("Action").w(px(36.0)).flex_shrink_0())
        }
    }

    fn render_inventory_rows(
        &self,
        d: DesignSystem,
        entries: &[ViewEntry],
        max_bytes: u64,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut rows = div()
            .id("inventory-rows")
            .track_scroll(&self.inventory_scroll)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pr(px(22.0));

        if entries.is_empty() {
            rows = rows.child(
                div()
                    .px(px(14.0))
                    .py(px(20.0))
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_tertiary)
                    .child("No artifacts available. Run a scan from the dashboard or browser."),
            );
        } else {
            for entry in entries {
                let index = entry.index;
                let expanded = self.expanded_rows.contains(&index);
                let row = self.render_inventory_row(d, entry, max_bytes, compact, expanded, cx);
                rows = rows.child(div().child(Self::separator(d)).child(row));
            }
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(rows)
            .child(Self::scroll_overlay(d, &self.inventory_scroll))
    }

    fn render_inventory_row(
        &self,
        d: DesignSystem,
        entry: &ViewEntry,
        max_bytes: u64,
        compact: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let index = entry.index;
        let path = entry.path.clone();
        let project_name = entry.project_name.clone();
        let size_bytes = entry.size_bytes;
        let selected = entry.selected;
        let is_orphaned = entry.is_orphaned;
        let dir_type = entry.dir_type;

        let filled = scaled_segments_from_max(size_bytes, max_bytes, 6);
        let size_color = rule_color(d, dir_type.rule.color_hint);
        let type_label = entry_type_label(dir_type);
        let status_label = if is_orphaned { "Orphaned" } else { type_label };

        let path_label = if expanded {
            path.clone()
        } else {
            truncate_end(&path, if compact { 48 } else { 62 })
        };

        let toggle_index = index;
        let action = Self::action_toggle(
            d,
            ElementId::Name(format!("toggle-{index}").into()),
            selected,
            cx.listener(move |this, _, _, cx| {
                this.app
                    .update(cx, |app, cx| app.toggle_selection(toggle_index, cx));
            }),
        );

        let chevron = div()
            .w(px(18.0))
            .h(px(18.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_size(d.typography.size_sm)
            .text_color(d.colors.text_tertiary)
            .child(if expanded { "▾" } else { "▸" }.to_string());

        let path_cell = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(if selected {
                        d.colors.accent_green
                    } else {
                        d.colors.text_primary
                    })
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(path_label),
            )
            .when(expanded || !project_name.is_empty(), |cell| {
                cell.child(
                    div()
                        .text_size(d.typography.size_xs)
                        .text_color(d.colors.text_secondary)
                        .child(if project_name.is_empty() {
                            format!("Type: {status_label}")
                        } else if compact {
                            format!("Type: {status_label} // {project_name}")
                        } else {
                            format!("Project: {project_name}")
                        }),
                )
            });

        let primary = if compact {
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(chevron)
                .child(path_cell)
                .child(
                    div()
                        .w(px(64.0))
                        .flex_shrink_0()
                        .text_size(d.typography.size_xs)
                        .text_color(d.colors.text_secondary)
                        .child(utils::format_size(size_bytes)),
                )
                .child(div().w(px(36.0)).flex_shrink_0().child(action))
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(chevron)
                .child(path_cell)
                .child(
                    div()
                        .w(px(112.0))
                        .flex_shrink_0()
                        .text_size(d.typography.size_xs)
                        .text_color(d.colors.text_secondary)
                        .child(status_label.to_string()),
                )
                .child(
                    div()
                        .w(px(72.0))
                        .flex_shrink_0()
                        .text_size(d.typography.size_xs)
                        .text_color(d.colors.text_secondary)
                        .child(utils::format_size(size_bytes)),
                )
                .child(div().w(px(96.0)).flex_shrink_0().child(Self::meter_bar(
                    d,
                    filled,
                    6,
                    size_color,
                    px(8.0),
                    px(8.0),
                )))
                .child(div().w(px(36.0)).flex_shrink_0().child(action))
        };

        let click_index = index;
        let mut row = div()
            .id(ElementId::Name(format!("inventory-{index}").into()))
            .px(px(8.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .bg(if selected {
                Gradients::cta_emphasized(&d.colors)
            } else {
                Gradients::cta_quiet(&d.colors)
            })
            .border_l_2()
            .border_color(if selected {
                d.colors.accent_green
            } else {
                alpha(d.colors.bg_primary, 0.0)
            })
            .cursor_pointer()
            .hover(|style| style.bg(alpha(d.colors.text_primary, 0.05)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_row_expanded(click_index, cx);
            }))
            .child(primary);

        if expanded {
            row = row.child(
                div()
                    .pl(px(30.0))
                    .pr(px(8.0))
                    .pt(px(2.0))
                    .pb(px(4.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .child(format!("Path: {}", path)),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_tertiary)
                            .child(format!(
                                "Status: {} // Size: {}",
                                status_label,
                                utils::format_size(size_bytes)
                            )),
                    ),
            );
        }

        row
    }

    #[allow(clippy::too_many_arguments)]
    fn results_sidebar(
        d: DesignSystem,
        total_size: u64,
        selected_size: u64,
        artifact_count: usize,
        selected_count: usize,
        risk_level: &str,
        error_msg: Option<&str>,
        deleted_count: usize,
        delete_mode: DeleteMode,
        is_deleting: bool,
        app: Entity<ArtifactApp>,
    ) -> Div {
        let action_enabled = selected_size > 0 && !is_deleting;
        let has_results = artifact_count > 0;
        let app_delete = app.clone();
        let app_select_all = app.clone();
        let app_deselect_all = app.clone();
        let action_label = if is_deleting {
            "Deleting..."
        } else {
            match delete_mode {
                DeleteMode::Trash => "Move To Trash",
                DeleteMode::Permanent => "Delete Permanently",
            }
        };
        let safety_copy = match delete_mode {
            DeleteMode::Trash => {
                "Selected artifacts will be moved to Trash so you can recover them later if needed."
            }
            DeleteMode::Permanent => {
                "Selected artifacts will be removed from disk immediately. This action cannot be undone."
            }
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .px(px(18.0))
            .pt(px(14.0))
            .pb(px(18.0))
            .child(
                div()
                    .flex_shrink_0()
                    .relative()
                    .border_1()
                    .border_color(d.colors.accent_green)
                    .rounded(d.radius.xs)
                    .bg(Gradients::cta_emphasized(&d.colors))
                    .px(px(16.0))
                    .py(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .absolute()
                            .top(px(-1.0))
                            .left(px(-1.0))
                            .right(px(-1.0))
                            .h(px(2.0))
                            .bg(Gradients::header_strip(&d.colors)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().w(px(6.0)).h(px(6.0)).bg(d.colors.accent_green))
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_secondary)
                                    .child("Total Selection"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(FontWeight::BLACK)
                            .text_color(d.colors.text_primary)
                            .child(utils::format_size(selected_size)),
                    ),
            )
            .child(Self::separator(d))
            .child(Self::results_metric_line(
                d,
                "Directories",
                &format_number(artifact_count),
            ))
            .child(Self::separator(d))
            .child(Self::results_metric_line(
                d,
                "Selected",
                &format_number(selected_count),
            ))
            .child(Self::separator(d))
            .child(Self::results_metric_line(d, "Risk_level", risk_level))
            .child(Self::separator(d))
            .child(Self::results_metric_line(
                d,
                "Last_scrub",
                if deleted_count == 0 {
                    "Unknown"
                } else {
                    "Recorded"
                },
            ))
            .child(Self::separator(d))
            .child(
                div()
                    .my(px(12.0))
                    .relative()
                    .border_1()
                    .border_color(d.colors.border_primary)
                    .rounded(d.radius.xs)
                    .bg(Gradients::cta_quiet(&d.colors))
                    .pl(px(16.0))
                    .pr(px(12.0))
                    .py(px(10.0))
                    .child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .bottom(px(0.0))
                            .w(px(3.0))
                            .bg(Gradients::accent_strip(d.colors.accent_green)),
                    )
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(d.typography.size_sm)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(d.colors.text_primary)
                            .child("Safety_Protocol"),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .child(safety_copy),
                    ),
            )
            .when(error_msg.is_some(), |panel| {
                panel.child(
                    div()
                        .mb(px(18.0))
                        .p(px(14.0))
                        .border_1()
                        .border_color(alpha(d.colors.accent_orange, 0.55))
                        .rounded(d.radius.xs)
                        .bg(Gradients::notice_glow(d.colors.accent_orange))
                        .child(
                            div()
                                .text_size(d.typography.size_xs)
                                .text_color(d.colors.accent_orange)
                                .child(error_msg.unwrap_or_default().to_string()),
                        ),
                )
            })
            .child(div().flex_1())
            .child(Self::separator(d))
            .child(
                div()
                    .pt(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child("Hash: 0X82F..91"),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child("Ref: [P2-V2]"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .child(format!(
                                "Total Space Identified: {}",
                                utils::format_size(total_size)
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::terminal_button_sm(
                                d,
                                "select-all-btn",
                                "Select All",
                                has_results,
                                move |_, _, cx| {
                                    app_select_all.update(cx, |app, cx| app.select_all_visible(cx));
                                },
                            ))
                            .child(Self::terminal_button_sm(
                                d,
                                "deselect-all-btn",
                                "Clear All",
                                action_enabled,
                                move |_, _, cx| {
                                    app_deselect_all.update(cx, |app, cx| app.deselect_all(cx));
                                },
                            )),
                    )
                    .child(Self::terminal_button(
                        d,
                        "purge-btn",
                        action_label,
                        action_enabled,
                        true,
                        move |_, _, cx| {
                            app_delete.update(cx, |app, cx| app.request_delete_confirm(cx));
                        },
                    )),
            )
    }

    fn results_metric_line(d: DesignSystem, label: &str, value: &str) -> Div {
        div()
            .py(px(14.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_secondary)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value.to_string()),
            )
    }

    fn toggle_row(
        d: DesignSystem,
        label: &str,
        checked: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_secondary)
                    .child(label.to_string()),
            )
            .child(Self::action_toggle(
                d,
                ElementId::Name(format!("toggle-{label}").into()),
                checked,
                on_click,
            ))
    }

    fn action_toggle(
        d: DesignSystem,
        id: impl Into<ElementId>,
        checked: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .w(px(38.0))
            .h(px(18.0))
            .rounded(d.radius.xs)
            .border_1()
            .border_color(if checked {
                d.colors.accent_green
            } else {
                alpha(d.colors.text_primary, 0.30)
            })
            .bg(if checked {
                Gradients::cta_emphasized(&d.colors)
            } else {
                Gradients::cta_quiet(&d.colors)
            })
            .flex()
            .items_center()
            .px(px(2.0))
            .cursor_pointer()
            .on_click(move |event, window, app| on_click(event, window, app))
            .child(
                div()
                    .w(px(12.0))
                    .h(px(10.0))
                    .bg(if checked {
                        d.colors.accent_green
                    } else {
                        d.colors.text_secondary
                    })
                    .when(checked, |thumb| thumb.ml(px(20.0))),
            )
    }

    fn terminal_button(
        d: DesignSystem,
        id: impl Into<ElementId>,
        label: &'static str,
        enabled: bool,
        emphasized: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let mut button = div()
            .id(id)
            .relative()
            .px(px(18.0))
            .py(px(14.0))
            .border_1()
            .border_color(if emphasized {
                d.colors.accent_green
            } else {
                d.colors.border_primary
            })
            .bg(if emphasized {
                Gradients::cta_emphasized(&d.colors)
            } else {
                Gradients::cta_quiet(&d.colors)
            })
            .rounded(d.radius.xs);

        if enabled {
            button = button
                .cursor_pointer()
                .hover(|style| style.bg(Gradients::cta_emphasized(&d.colors)))
                .active(|style| style.bg(alpha(d.colors.text_primary, 0.12)))
                .on_click(move |event, window, app| on_click(event, window, app));
        }

        if emphasized {
            button = button.child(
                div()
                    .absolute()
                    .top(px(-1.0))
                    .left(px(-1.0))
                    .right(px(-1.0))
                    .h(px(1.0))
                    .bg(Gradients::header_strip(&d.colors)),
            );
        }

        button.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .when(emphasized, |row| {
                    row.child(div().w(px(4.0)).h(px(4.0)).bg(d.colors.accent_green))
                })
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::BLACK)
                        .text_color(if enabled {
                            d.colors.text_primary
                        } else {
                            d.colors.text_tertiary
                        })
                        .child(label),
                ),
        )
    }

    fn terminal_button_sm(
        d: DesignSystem,
        id: impl Into<ElementId>,
        label: &'static str,
        enabled: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let mut button = div()
            .id(id)
            .px(px(10.0))
            .py(px(4.0))
            .border_1()
            .border_color(d.colors.border_primary)
            .bg(Gradients::cta_quiet(&d.colors))
            .rounded(d.radius.xs);

        if enabled {
            button = button
                .cursor_pointer()
                .hover(|style| style.bg(Gradients::cta_emphasized(&d.colors)))
                .active(|style| style.bg(alpha(d.colors.text_primary, 0.12)))
                .on_click(move |event, window, app| on_click(event, window, app));
        }

        button.child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::BLACK)
                .text_color(if enabled {
                    d.colors.text_primary
                } else {
                    d.colors.text_tertiary
                })
                .child(label),
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

fn format_unix_time(ts: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(ts);

    let delta = now - ts;
    let when = if delta < 60 {
        "just now".to_string()
    } else if delta < 3_600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3_600)
    } else if delta < 604_800 {
        format!("{}d ago", delta / 86_400)
    } else {
        format!("{}w ago", delta / 604_800)
    };

    format!("RUN @ {} // {}", ts, when)
}

fn sidebar_icon_name(icon: SidebarIcon) -> &'static str {
    match icon {
        SidebarIcon::Dashboard => "dashboard",
        SidebarIcon::Results => "results",
        SidebarIcon::Browser => "browser",
        SidebarIcon::History => "history",
        SidebarIcon::Settings => "settings",
    }
}

fn summarize_languages(
    d: DesignSystem,
    enabled_rule_names: &[(&'static str, bool)],
) -> Vec<LanguageSetting> {
    let mut grouped: BTreeMap<&'static str, (usize, usize, Hsla)> = BTreeMap::new();

    for (rule_name, enabled) in enabled_rule_names {
        let Some(rule) = rules::find(rule_name) else {
            continue;
        };

        let entry = grouped
            .entry(rule.language)
            .or_insert((0, 0, rule_color(d, rule.color_hint)));
        if *enabled {
            entry.0 += 1;
        }
        entry.1 += 1;
    }

    let mut ordered = Vec::new();
    for rule in rules::RULES {
        if let Some((enabled_count, total_count, color)) = grouped.remove(rule.language) {
            ordered.push(LanguageSetting {
                label: rule.language,
                enabled: enabled_count == total_count,
                enabled_count,
                total_count,
                color,
            });
        }
    }

    ordered
}

fn summarize_artifacts(entries: &[ViewEntry]) -> Vec<ArtifactBucket> {
    let mut buckets = BTreeMap::<String, u64>::new();
    for entry in entries {
        *buckets
            .entry(entry.dir_type.rule.language.to_uppercase())
            .or_default() += entry.size_bytes;
    }

    let mut items: Vec<_> = buckets
        .into_iter()
        .map(|(label, size_bytes)| ArtifactBucket { label, size_bytes })
        .collect();
    items.sort_by_key(|bucket| Reverse(bucket.size_bytes));
    items
}

fn summary_windows(buckets: &[ArtifactBucket]) -> Vec<ArtifactBucket> {
    let mut out: Vec<_> = buckets
        .iter()
        .take(4)
        .enumerate()
        .map(|(index, bucket)| ArtifactBucket {
            label: format!("W{}", index + 1),
            size_bytes: bucket.size_bytes,
        })
        .collect();

    while out.len() < 4 {
        out.push(ArtifactBucket {
            label: format!("W{}", out.len() + 1),
            size_bytes: 0,
        });
    }

    out
}

fn scaled_segments(bucket_size: u64, buckets: &[ArtifactBucket], max_segments: usize) -> usize {
    let max = buckets
        .iter()
        .map(|bucket| bucket.size_bytes)
        .max()
        .unwrap_or(1);
    scaled_segments_from_max(bucket_size, max, max_segments)
}

fn scaled_segments_from_max(size: u64, max: u64, max_segments: usize) -> usize {
    if size == 0 || max == 0 {
        1
    } else {
        (((size as f32 / max as f32) * max_segments as f32).ceil() as usize).clamp(1, max_segments)
    }
}

fn entry_type_label(dir_type: DirectoryType) -> &'static str {
    match dir_type.rule.name {
        "rust_target" => "Build Output",
        "python_venv" | "python_venv_alt" => "Python Venv",
        "pycache" => "Python",
        "next_cache" => "NextJS",
        "composer_vendor" => "Vendor",
        "node_modules" => "NodeJS",
        _ => dir_type.rule.language,
    }
}

fn truncate_end(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}...", &text[..max.saturating_sub(3)])
    }
}

fn format_number(n: usize) -> String {
    if n < 1_000 {
        return n.to_string();
    }

    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
