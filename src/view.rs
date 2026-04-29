use gpui::prelude::FluentBuilder;
use gpui::*;
use std::path::PathBuf;
use std::time::Duration;

use crate::app::{ArtifactApp, ScanState};
use artifact::components::*;
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

pub struct ArtifactView {
    app: Entity<ArtifactApp>,
    design: DesignSystem,
    active_view: SidebarView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarView {
    Overview,
    Browser,
    Activity,
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
                    app_clone.update(cx, |app, cx| app.check_scan_progress(cx));
                });
            }
        })
        .detach();

        Self {
            app,
            design: DesignSystem::new(),
            active_view: SidebarView::Overview,
        }
    }
}

// ---------------------------------------------------------------------------
// Root render
// ---------------------------------------------------------------------------

impl Render for ArtifactView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let d = self.design;

        let scan_state = app.scan_state();
        let is_scanning = scan_state == ScanState::Scanning;
        let scan_path = app.scan_path().to_string();
        let total_size = app.total_size();
        let selected_size = app.selected_size();
        let deleted_count = app.deleted_count();
        let error_msg = app.error_message().map(|s| s.to_string());
        let enabled_rule_names: Vec<(&'static str, bool)> = rules::RULES
            .iter()
            .map(|r| (r.name, app.is_rule_enabled(r.name)))
            .collect();
        let enabled_rule_count = app.enabled_rule_count();
        let show_orphaned = app.show_orphaned_only();
        let progress = app.scan_progress_data().cloned();
        let file_browser_open = app.is_file_browser_open();
        let browse_path = app.browse_path().display().to_string();
        let browse_entries: Vec<_> = app
            .browse_entries()
            .iter()
            .map(|e| (e.name.clone(), e.path.clone()))
            .collect();
        let scan_log: Vec<String> = app.scan_log.iter().rev().take(50).cloned().collect();

        let dir_entries: Vec<_> = app
            .visible_entries()
            .iter()
            .map(|(i, item)| {
                (
                    *i,
                    item.path.display().to_string(),
                    item.dir_type.clone(),
                    item.project_name.clone().unwrap_or_default(),
                    item.size_bytes,
                    item.selected,
                    item.is_orphaned,
                )
            })
            .collect();
        let has_entries = !dir_entries.is_empty();
        let visible_count = dir_entries.len();
        let total_count = app.visible_entries().len();
        let max_bytes: u64 = dir_entries.iter().map(|e| e.4).max().unwrap_or(1).max(1);
        let active_view = if file_browser_open {
            SidebarView::Browser
        } else {
            self.active_view
        };
        let _ = app;

        // Cloned handles for closures
        let app_scan = self.app.clone();
        let app_sel_all = self.app.clone();
        let app_sel_none = self.app.clone();
        let app_delete = self.app.clone();
        let app_orph = self.app.clone();
        let app_browse_open = self.app.clone();

        // Root: bento canvas — padded background, sans body, sidebar + content cards with a gutter
        div()
            .size_full()
            .font_family("Helvetica")
            .bg(d.colors.bg_primary)
            .text_color(d.colors.text_primary)
            .p(d.spacing.md)
            .gap(d.spacing.md)
            .flex()
            .flex_row()
            // Sidebar (card)
            .child(self.render_sidebar(d, active_view, scan_state, cx))
            // Content area: topbar + panels (card)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(d.spacing.md)
                    // Topbar
                    .child(Self::render_topbar(
                        d,
                        scan_state,
                        total_size,
                        visible_count,
                        active_view,
                    ))
                    // Body panels
                    .child(match active_view {
                        SidebarView::Overview => {
                            if is_scanning {
                                Self::render_scan_view(d, progress.as_ref(), &scan_log)
                            } else {
                                Self::render_idle_view(
                                    d,
                                    scan_state,
                                    &scan_path,
                                    &enabled_rule_names,
                                    show_orphaned,
                                    &dir_entries,
                                    has_entries,
                                    visible_count,
                                    total_count,
                                    total_size,
                                    selected_size,
                                    deleted_count,
                                    error_msg.as_deref(),
                                    file_browser_open,
                                    &browse_path,
                                    &browse_entries,
                                    max_bytes,
                                    &self.app,
                                    app_scan,
                                    app_sel_all,
                                    app_sel_none,
                                    app_delete,
                                    app_orph,
                                    app_browse_open,
                                )
                            }
                        }
                        SidebarView::Browser => Self::render_browser_view(
                            d,
                            &scan_path,
                            &browse_path,
                            &browse_entries,
                            file_browser_open,
                            enabled_rule_count,
                            show_orphaned,
                            &self.app,
                        ),
                        SidebarView::Activity => Self::render_activity_view(
                            d,
                            scan_state,
                            progress.as_ref(),
                            &scan_log,
                            &scan_path,
                            total_size,
                            visible_count,
                            selected_size,
                        ),
                    }),
            )
    }
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn activate_view(&mut self, view: SidebarView, cx: &mut Context<Self>) {
        self.active_view = view;
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

    fn render_sidebar(
        &self,
        d: DesignSystem,
        active_view: SidebarView,
        scan_state: ScanState,
        cx: &mut Context<Self>,
    ) -> Div {
        let status_label = match scan_state {
            ScanState::Idle => "Ready",
            ScanState::Scanning => "Scan active",
            ScanState::Complete => "Results ready",
        };
        let status_color = match scan_state {
            ScanState::Idle => d.colors.text_tertiary,
            ScanState::Scanning => d.colors.accent_orange,
            ScanState::Complete => d.colors.accent_green,
        };

        div()
            .w(px(220.0))
            .flex_shrink_0()
            .h_full()
            .bg(d.colors.bg_secondary)
            .rounded(d.radius.md)
            .flex()
            .flex_col()
            .items_start()
            .overflow_hidden()
            // Logo mark
            .child(
                div()
                    .w_full()
                    .h(px(72.0))
                    .flex()
                    .items_center()
                    .gap(d.spacing.sm)
                    .px(d.spacing.lg)
                    .child(
                        div()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded(d.radius.sm)
                            .bg(d.colors.accent_green)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .text_color(d.colors.bg_primary)
                                    .text_size(d.typography.size_md)
                                    .font_weight(FontWeight::BOLD)
                                    .child("A"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_color(d.colors.text_primary)
                                    .text_size(d.typography.size_lg)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Artifact"),
                            )
                            .child(
                                div()
                                    .text_color(d.colors.text_tertiary)
                                    .text_size(d.typography.size_xs)
                                    .child("Space reclaim"),
                            ),
                    ),
            )
            // Nav
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(4.0))
                    .px(d.spacing.sm)
                    .pt(d.spacing.sm)
                    .child(Self::sidebar_nav_item(
                        d,
                        "Overview",
                        "Scan + results",
                        active_view == SidebarView::Overview,
                        cx.listener(|this, _, _, cx| {
                            this.activate_view(SidebarView::Overview, cx);
                        }),
                    ))
                    .child(Self::sidebar_nav_item(
                        d,
                        "Browser",
                        "Target directory",
                        active_view == SidebarView::Browser,
                        cx.listener(|this, _, _, cx| {
                            this.open_browser_view(cx);
                        }),
                    ))
                    .child(Self::sidebar_nav_item(
                        d,
                        "Activity",
                        "Live scan log",
                        active_view == SidebarView::Activity,
                        cx.listener(|this, _, _, cx| {
                            this.activate_view(SidebarView::Activity, cx);
                        }),
                    )),
            )
            // Status at bottom
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .p(d.spacing.sm)
                    .child(
                        div()
                            .w_full()
                            .p(d.spacing.md)
                            .rounded(d.radius.sm)
                            .bg(d.colors.bg_tertiary)
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(Self::panel_label(d, "SYSTEM_STATE"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div().w(px(8.0)).h(px(8.0)).rounded_full().bg(status_color),
                                    )
                                    .child(
                                        div()
                                            .text_size(d.typography.size_sm)
                                            .text_color(d.colors.text_primary)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(status_label),
                                    ),
                            ),
                    ),
            )
    }

    fn sidebar_nav_item(
        d: DesignSystem,
        label: &'static str,
        subtitle: &'static str,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(ElementId::Name(format!("nav-{}", label).into()))
            .w_full()
            .px(d.spacing.md)
            .py(d.spacing.sm)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(d.radius.sm)
            .bg(if active {
                d.colors.bg_tertiary
            } else {
                Hsla {
                    a: 0.0,
                    ..d.colors.bg_secondary
                }
            })
            .hover(|s| s.bg(d.colors.interactive_hover))
            .active(|s| s.bg(d.colors.interactive_active))
            .cursor_pointer()
            .on_click(move |event, window, cx| on_click(event, window, cx))
            .child(
                div()
                    .text_size(d.typography.size_md)
                    .text_color(if active {
                        d.colors.text_primary
                    } else {
                        d.colors.text_secondary
                    })
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .child(label),
            )
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(subtitle),
            )
    }
}

// ---------------------------------------------------------------------------
// Topbar
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_topbar(
        d: DesignSystem,
        scan_state: ScanState,
        total_size: u64,
        item_count: usize,
        active_view: SidebarView,
    ) -> Div {
        let view_label = match active_view {
            SidebarView::Overview => "Overview",
            SidebarView::Browser => "Browser",
            SidebarView::Activity => "Activity",
        };
        let status_label = match scan_state {
            ScanState::Idle => "Idle",
            ScanState::Scanning => "Scanning",
            ScanState::Complete => "Complete",
        };
        let status_color = match scan_state {
            ScanState::Idle => d.colors.text_tertiary,
            ScanState::Scanning => d.colors.accent_orange,
            ScanState::Complete => d.colors.accent_green,
        };

        div()
            .flex_shrink_0()
            .h(px(64.0))
            .px(d.spacing.lg)
            .flex()
            .items_center()
            .gap(d.spacing.lg)
            .bg(d.colors.bg_secondary)
            .rounded(d.radius.md)
            // Brand
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(d.typography.size_xl)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(d.colors.text_primary)
                            .child(view_label),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_tertiary)
                            .child("Space reclaim workbench"),
                    ),
            )
            // Spacer
            .child(div().flex_1())
            // Right stats
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(d.spacing.lg)
                    .child(Self::topbar_stat(d, "Status", status_label, status_color))
                    .child(Self::topbar_stat(
                        d,
                        "Items",
                        &item_count.to_string(),
                        d.colors.text_primary,
                    ))
                    .child(Self::topbar_stat(
                        d,
                        "Total",
                        &utils::format_size(total_size),
                        d.colors.text_primary,
                    )),
            )
    }

    fn topbar_stat(d: DesignSystem, label: &'static str, value: &str, value_color: Hsla) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(prettify_label(label)),
            )
            .child(
                div()
                    .text_size(d.typography.size_md)
                    .text_color(value_color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value.to_string()),
            )
    }
}

// ---------------------------------------------------------------------------
// Idle / Complete view  (left list + right stats)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
impl ArtifactView {
    fn render_idle_view(
        d: DesignSystem,
        scan_state: ScanState,
        scan_path: &str,
        enabled_rule_names: &[(&'static str, bool)],
        show_orphaned: bool,
        dir_entries: &[(usize, String, DirectoryType, String, u64, bool, bool)],
        has_entries: bool,
        visible_count: usize,
        total_count: usize,
        total_size: u64,
        selected_size: u64,
        deleted_count: usize,
        error_msg: Option<&str>,
        file_browser_open: bool,
        browse_path: &str,
        browse_entries: &[(String, PathBuf)],
        max_bytes: u64,
        app: &Entity<ArtifactApp>,
        app_scan: Entity<ArtifactApp>,
        app_sel_all: Entity<ArtifactApp>,
        app_sel_none: Entity<ArtifactApp>,
        app_delete: Entity<ArtifactApp>,
        app_orph: Entity<ArtifactApp>,
        app_browse_open: Entity<ArtifactApp>,
    ) -> Div {
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .gap(d.spacing.md)
            // ── Left panel (scan config + artifact list) ──────────────────
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .overflow_hidden()
                    // SCAN_CONFIG section
                    .child(
                        div()
                            .flex_shrink_0()
                            .p(d.spacing.lg)
                            .flex()
                            .flex_col()
                            .gap(d.spacing.md)
                            .child(Self::panel_label(d, "SCAN_CONFIG"))
                            // Path + browse
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(d.spacing.sm)
                                    .child(
                                        div().flex_1().child(
                                            Input::new("SCAN_PATH...", scan_path, d).render(),
                                        ),
                                    )
                                    .child(
                                        Button::new("Browse", d)
                                            .variant(ButtonVariant::Secondary)
                                            .render("btn-browse", move |_, _, cx| {
                                                app_browse_open
                                                    .update(cx, |a, cx| a.open_file_browser(cx));
                                            }),
                                    ),
                            )
                            // Rule chip-toggle row (wraps)
                            .child(Self::render_rule_chips(d, enabled_rule_names, app))
                            // Orphaned-only filter
                            .child(
                                div().flex().items_center().child(
                                    Checkbox::new("orphaned only", show_orphaned, d).render(
                                        "cb-orph",
                                        move |_, _, cx| {
                                            app_orph
                                                .update(cx, |a, cx| a.toggle_orphaned_only(cx));
                                        },
                                    ),
                                ),
                            )
                            // Scan button
                            .child(Button::new("Scan", d).render("btn-scan", move |_, _, cx| {
                                app_scan.update(cx, |a, cx| a.start_scan(cx));
                            })),
                    )
                    // File browser (replaces artifact list when open)
                    .when(file_browser_open, |root| {
                        root.child(Self::render_file_browser(
                            d,
                            browse_path,
                            browse_entries,
                            app,
                        ))
                    })
                    // ARTIFACT_DIRECTORY list
                    .when(!file_browser_open, |root| {
                        root.child(Self::render_artifact_list(
                            d,
                            scan_state,
                            dir_entries,
                            has_entries,
                            visible_count,
                            total_count,
                            max_bytes,
                            app,
                            app_sel_all,
                            app_sel_none,
                        ))
                    }),
            )
            // ── Right panel (stats + controls) ───────────────────────────
            .child(Self::render_stats_panel(
                d,
                scan_state,
                visible_count,
                total_size,
                selected_size,
                deleted_count,
                error_msg,
                has_entries,
                app_delete,
            ))
    }
}

impl ArtifactView {
    fn render_rule_chips(
        d: DesignSystem,
        enabled_rule_names: &[(&'static str, bool)],
        app: &Entity<ArtifactApp>,
    ) -> Div {
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(6.0));

        for (name, enabled) in enabled_rule_names {
            let Some(rule) = rules::find(name) else {
                continue;
            };
            let color = rule_color(d, rule.color_hint);
            let enabled = *enabled;
            let app_chip = app.clone();
            let rule_name = rule.name;

            let chip = div()
                .id(ElementId::Name(format!("chip-{}", rule_name).into()))
                .px(d.spacing.md)
                .py(px(6.0))
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0))
                .bg(if enabled {
                    Hsla { a: 0.16, ..color }
                } else {
                    d.colors.bg_tertiary
                })
                .border_1()
                .border_color(if enabled {
                    Hsla { a: 0.45, ..color }
                } else {
                    d.colors.border_secondary
                })
                .hover(|s| s.bg(d.colors.interactive_hover))
                .on_click(move |_, _, cx| {
                    app_chip.update(cx, |a, cx| a.toggle_rule(rule_name, cx));
                })
                .child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded_full()
                        .bg(if enabled { color } else { d.colors.text_tertiary }),
                )
                .child(
                    div()
                        .text_size(d.typography.size_xs)
                        .text_color(if enabled {
                            d.colors.text_primary
                        } else {
                            d.colors.text_tertiary
                        })
                        .font_weight(if enabled {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .child(rule.language),
                );

            row = row.child(chip);
        }

        row
    }
}

// ---------------------------------------------------------------------------
// Artifact directory list
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_artifact_list(
        d: DesignSystem,
        scan_state: ScanState,
        entries: &[(usize, String, DirectoryType, String, u64, bool, bool)],
        has_entries: bool,
        visible_count: usize,
        total_count: usize,
        max_bytes: u64,
        app: &Entity<ArtifactApp>,
        app_sel_all: Entity<ArtifactApp>,
        app_sel_none: Entity<ArtifactApp>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            // Header row
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(d.spacing.lg)
                    .py(d.spacing.md)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(d.spacing.sm)
                            .child(Self::panel_label(d, "ARTIFACT_DIRECTORY"))
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child(format!("{} of {}", visible_count, total_count)),
                            ),
                    )
                    .when(has_entries, |row| {
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(Button::new("All", d).variant(ButtonVariant::Ghost).render(
                                    "btn-sel-all",
                                    move |_, _, cx| {
                                        app_sel_all.update(cx, |a, cx| a.select_all(cx));
                                    },
                                ))
                                .child(
                                    Button::new("None", d).variant(ButtonVariant::Ghost).render(
                                        "btn-sel-none",
                                        move |_, _, cx| {
                                            app_sel_none.update(cx, |a, cx| a.select_none(cx));
                                        },
                                    ),
                                ),
                        )
                    }),
            )
            // Scrollable list
            .child(Self::render_dir_rows(
                d, scan_state, entries, max_bytes, app,
            ))
    }

    fn render_dir_rows(
        d: DesignSystem,
        scan_state: ScanState,
        entries: &[(usize, String, DirectoryType, String, u64, bool, bool)],
        max_bytes: u64,
        app: &Entity<ArtifactApp>,
    ) -> Stateful<Div> {
        let mut list = div()
            .id("dir-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(d.spacing.md)
            .pb(d.spacing.md)
            .gap(px(4.0));

        if entries.is_empty() {
            list = list.child(
                div()
                    .p(d.spacing.md)
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_sm)
                    .child(match scan_state {
                        ScanState::Idle => "Run a scan to find build artifacts",
                        ScanState::Scanning => "Scanning…",
                        ScanState::Complete => "No artifacts found",
                    }),
            );
        } else {
            for (idx, path, dir_type, project_name, size_bytes, selected, is_orphaned) in entries {
                let app_toggle = app.clone();
                let idx = *idx;
                let size_bytes = *size_bytes;
                let selected = *selected;
                let is_orphaned = *is_orphaned;

                let badge_color = rule_color(d, dir_type.rule.color_hint);
                let badge_label = dir_type.rule.language.to_string();
                let size_str = utils::format_size(size_bytes);

                // Compute filled block count (0-5) proportional to max
                let filled = ((size_bytes as f64 / max_bytes as f64) * 5.0).ceil() as usize;
                let filled = filled.clamp(1, 5);

                list = list.child(
                    div()
                        .id(ElementId::Name(format!("dir-{idx}").into()))
                        .flex()
                        .items_center()
                        .px(d.spacing.md)
                        .py(d.spacing.sm)
                        .gap(d.spacing.sm)
                        .rounded(d.radius.sm)
                        .cursor_pointer()
                        .when(selected, |el| {
                            el.bg(Hsla {
                                a: 0.14,
                                ..d.colors.accent_green
                            })
                        })
                        .hover(|s| s.bg(d.colors.interactive_hover))
                        .on_click(move |_, _, cx| {
                            app_toggle.update(cx, |a, cx| a.toggle_selection(idx, cx));
                        })
                        // Selected indicator bar
                        .child(
                            div()
                                .w(px(3.0))
                                .h(px(32.0))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(if selected {
                                    d.colors.accent_green
                                } else {
                                    d.colors.border_secondary
                                }),
                        )
                        // Path + info
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .font_family("Menlo")
                                        .text_size(d.typography.size_sm)
                                        .text_color(if selected {
                                            d.colors.text_primary
                                        } else {
                                            d.colors.text_secondary
                                        })
                                        .overflow_x_hidden()
                                        .child(truncate_path(path, 60)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(d.spacing.sm)
                                        .when(!project_name.is_empty(), |row| {
                                            row.child(
                                                div()
                                                    .text_size(d.typography.size_xs)
                                                    .text_color(d.colors.text_tertiary)
                                                    .child(project_name.clone()),
                                            )
                                        })
                                        .when(is_orphaned, |row| {
                                            row.child(
                                                div()
                                                    .text_size(d.typography.size_xs)
                                                    .text_color(d.colors.accent_orange)
                                                    .child("Orphaned"),
                                            )
                                        }),
                                ),
                        )
                        // Size value
                        .child(
                            div()
                                .font_family("Menlo")
                                .text_size(d.typography.size_sm)
                                .text_color(badge_color)
                                .font_weight(FontWeight::SEMIBOLD)
                                .flex_shrink_0()
                                .child(size_str),
                        )
                        // Block-size bar
                        .child(size_blocks(filled, badge_color, d))
                        // Type badge
                        .child(Badge::new(badge_label, badge_color, d).render()),
                );
            }
        }

        list
    }
}

// ---------------------------------------------------------------------------
// Stats panel (right)
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_stats_panel(
        d: DesignSystem,
        scan_state: ScanState,
        visible_count: usize,
        total_size: u64,
        selected_size: u64,
        deleted_count: usize,
        error_msg: Option<&str>,
        has_entries: bool,
        app_delete: Entity<ArtifactApp>,
    ) -> Div {
        let status_label = match scan_state {
            ScanState::Idle => "Ready to scan",
            ScanState::Scanning => "Scan running",
            ScanState::Complete => "Results ready",
        };
        let status_color = match scan_state {
            ScanState::Idle => d.colors.text_tertiary,
            ScanState::Scanning => d.colors.accent_orange,
            ScanState::Complete => d.colors.accent_green,
        };

        div()
            .w(px(280.0))
            .flex_shrink_0()
            .h_full()
            .bg(d.colors.bg_secondary)
            .rounded(d.radius.md)
            .overflow_hidden()
            .flex()
            .flex_col()
            // Header
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(48.0))
                    .flex()
                    .items_center()
                    .px(d.spacing.lg)
                    .child(Self::panel_label(d, "SYSTEM_RESULTS")),
            )
            // Total reclaimable (hero stat)
            .child(
                div()
                    .flex_shrink_0()
                    .mx(d.spacing.md)
                    .mb(d.spacing.md)
                    .p(d.spacing.lg)
                    .rounded(d.radius.sm)
                    .bg(Gradients::green_card(&d.colors))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(Hsla {
                                a: 0.65,
                                ..d.colors.accent_green
                            })
                            .child("Total reclaimable"),
                    )
                    .child(
                        div()
                            .font_family("Helvetica")
                            .text_size(d.typography.size_title)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(d.colors.text_primary)
                            .child(utils::format_size(total_size)),
                    ),
            )
            // Stats rows
            .child(
                div()
                    .flex_shrink_0()
                    .px(d.spacing.lg)
                    .pb(d.spacing.md)
                    .flex()
                    .flex_col()
                    .gap(d.spacing.sm)
                    .child(Self::stat_row(
                        d,
                        "SELECTED_SIZE",
                        &utils::format_size(selected_size),
                        d.colors.accent_green,
                    ))
                    .child(Self::stat_row(
                        d,
                        "ITEMS_DELETED",
                        &deleted_count.to_string(),
                        d.colors.text_secondary,
                    ))
                    .child(Self::stat_row(
                        d,
                        "ITEMS_FOUND",
                        &visible_count.to_string(),
                        d.colors.text_secondary,
                    )),
            )
            // Status indicator
            .child(
                div()
                    .flex_shrink_0()
                    .mx(d.spacing.lg)
                    .px(d.spacing.md)
                    .py(d.spacing.sm)
                    .rounded_full()
                    .bg(Hsla {
                        a: 0.10,
                        ..status_color
                    })
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(status_color))
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(status_color)
                            .font_weight(FontWeight::MEDIUM)
                            .child(status_label),
                    ),
            )
            // Spacer
            .child(div().flex_1())
            // Error
            .when(error_msg.is_some(), |panel| {
                let msg = error_msg.unwrap_or_default();
                panel.child(
                    div()
                        .flex_shrink_0()
                        .mx(d.spacing.lg)
                        .mb(d.spacing.sm)
                        .p(d.spacing.md)
                        .rounded(d.radius.sm)
                        .bg(Hsla {
                            a: 0.10,
                            ..d.colors.accent_orange
                        })
                        .child(
                            div()
                                .text_size(d.typography.size_xs)
                                .text_color(d.colors.accent_orange)
                                .child(msg.to_string()),
                        ),
                )
            })
            // Delete button
            .child(
                div().flex_shrink_0().p(d.spacing.lg).child(
                    Button::new("Delete Selected", d)
                        .variant(ButtonVariant::Danger)
                        .disabled(selected_size == 0 || !has_entries)
                        .render("btn-delete", move |_, _, cx| {
                            app_delete.update(cx, |a, cx| a.delete_selected(cx));
                        }),
                ),
            )
    }

    fn stat_row(d: DesignSystem, label: &'static str, value: &str, value_color: Hsla) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(prettify_label(label)),
            )
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(value_color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value.to_string()),
            )
    }
}

// ---------------------------------------------------------------------------
// Browser view
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_browser_view(
        d: DesignSystem,
        scan_path: &str,
        browse_path: &str,
        browse_entries: &[(String, PathBuf)],
        file_browser_open: bool,
        enabled_rule_count: usize,
        show_orphaned: bool,
        app: &Entity<ArtifactApp>,
    ) -> Div {
        let app_open = app.clone();

        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .gap(d.spacing.md)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .overflow_hidden()
                    .when(file_browser_open, |root| {
                        root.child(Self::render_file_browser(
                            d,
                            browse_path,
                            browse_entries,
                            app,
                        ))
                    })
                    .when(!file_browser_open, |root| {
                        root.child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .items_center()
                                .justify_center()
                                .gap(d.spacing.md)
                                .child(
                                    div()
                                        .text_size(d.typography.size_xl)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(d.colors.text_primary)
                                        .child("Directory browser"),
                                )
                                .child(
                                    div()
                                        .text_size(d.typography.size_sm)
                                        .text_color(d.colors.text_tertiary)
                                        .child("Open the browser to change the root scan path."),
                                )
                                .child(
                                    Button::new("Open Browser", d)
                                        .variant(ButtonVariant::Secondary)
                                        .render("btn-browser-open", move |_, _, cx| {
                                            app_open.update(cx, |a, cx| a.open_file_browser(cx));
                                        }),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(280.0))
                    .flex_shrink_0()
                    .h_full()
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px(d.spacing.lg)
                            .child(Self::panel_label(d, "BROWSER_CONTEXT")),
                    )
                    .child(
                        div()
                            .px(d.spacing.lg)
                            .pb(d.spacing.lg)
                            .flex()
                            .flex_col()
                            .gap(d.spacing.md)
                            .child(Self::stat_row(
                                d,
                                "SCAN_ROOT",
                                &truncate_path(scan_path, 24),
                                d.colors.text_primary,
                            ))
                            .child(Self::stat_row(
                                d,
                                "BROWSE_PATH",
                                &truncate_path(browse_path, 24),
                                d.colors.text_secondary,
                            ))
                            .child(Self::stat_row(
                                d,
                                "RULES_ENABLED",
                                &format!("{} of {}", enabled_rule_count, rules::RULES.len()),
                                if enabled_rule_count > 0 {
                                    d.colors.accent_green
                                } else {
                                    d.colors.text_tertiary
                                },
                            ))
                            .child(Self::stat_row(
                                d,
                                "ORPHAN_FILTER",
                                if show_orphaned { "Only" } else { "All" },
                                if show_orphaned {
                                    d.colors.accent_orange
                                } else {
                                    d.colors.text_secondary
                                },
                            )),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Activity view
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_activity_view(
        d: DesignSystem,
        scan_state: ScanState,
        progress: Option<&crate::app::ScanProgress>,
        scan_log: &[String],
        scan_path: &str,
        total_size: u64,
        visible_count: usize,
        selected_size: u64,
    ) -> Div {
        let (dirs, items, current_path, elapsed) = match progress {
            Some(p) => (
                p.directories_scanned,
                p.items_found,
                p.current_path.clone(),
                p.elapsed_secs,
            ),
            None => (0, visible_count, String::new(), 0.0),
        };

        let state_label = match scan_state {
            ScanState::Idle => "Idle",
            ScanState::Scanning => "Scanning",
            ScanState::Complete => "Complete",
        };
        let state_color = match scan_state {
            ScanState::Idle => d.colors.text_tertiary,
            ScanState::Scanning => d.colors.accent_orange,
            ScanState::Complete => d.colors.accent_green,
        };

        let mut recent_log = div()
            .id("activity-log")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .p(d.spacing.sm)
            .gap(px(1.0));

        if scan_log.is_empty() {
            recent_log = recent_log.child(
                div()
                    .p(d.spacing.sm)
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_tertiary)
                    .child("No scan activity yet"),
            );
        } else {
            recent_log = recent_log.children(scan_log.iter().map(|path| {
                div()
                    .font_family("Menlo")
                    .px(d.spacing.sm)
                    .py(px(4.0))
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_secondary)
                    .overflow_x_hidden()
                    .child(truncate_path(path, 46))
            }));
        }

        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .gap(d.spacing.md)
            .child(
                div()
                    .w(px(280.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px(d.spacing.lg)
                            .child(Self::panel_label(d, "SCAN_ACTIVITY")),
                    )
                    .child(
                        div()
                            .px(d.spacing.lg)
                            .pb(d.spacing.lg)
                            .flex()
                            .flex_col()
                            .gap(d.spacing.sm)
                            .child(Self::stat_row(d, "STATE", state_label, state_color))
                            .child(Self::stat_row(
                                d,
                                "TARGET",
                                &truncate_path(scan_path, 26),
                                d.colors.text_secondary,
                            ))
                            .child(Self::stat_row(
                                d,
                                "DIRS_SCANNED",
                                &format_number(dirs),
                                d.colors.text_secondary,
                            ))
                            .child(Self::stat_row(
                                d,
                                "ITEMS_FOUND",
                                &items.to_string(),
                                d.colors.accent_green,
                            ))
                            .child(Self::stat_row(
                                d,
                                "RECLAIMABLE",
                                &utils::format_size(total_size),
                                d.colors.text_primary,
                            ))
                            .child(Self::stat_row(
                                d,
                                "SELECTED",
                                &utils::format_size(selected_size),
                                d.colors.text_secondary,
                            ))
                            .child(Self::stat_row(
                                d,
                                "ELAPSED",
                                &utils::format_elapsed(elapsed),
                                d.colors.text_tertiary,
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(d.spacing.md)
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .child(
                        div()
                            .text_size(d.typography.size_title)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(state_color)
                            .child(state_label),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_sm)
                            .text_color(d.colors.text_tertiary)
                            .child(if current_path.is_empty() {
                                "Run a scan to stream directory activity here".to_string()
                            } else {
                                truncate_path(&current_path, 72)
                            }),
                    )
                    .when(scan_state == ScanState::Scanning, |panel| {
                        panel.child(
                            div()
                                .w(px(280.0))
                                .child(ProgressBar::new(d).render_indeterminate()),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(340.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px(d.spacing.lg)
                            .child(Self::panel_label(d, "RECENT_PATHS")),
                    )
                    .child(recent_log),
            )
    }
}

// ---------------------------------------------------------------------------
// Scanning view (breakdown | gauge | log)
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_scan_view(
        d: DesignSystem,
        progress: Option<&crate::app::ScanProgress>,
        scan_log: &[String],
    ) -> Div {
        let (dirs, items, current_path, size_found, elapsed) = match progress {
            Some(p) => (
                p.directories_scanned,
                p.items_found,
                p.current_path.clone(),
                p.total_size_found,
                p.elapsed_secs,
            ),
            None => (0, 0, String::new(), 0, 0.0),
        };

        let rate = if elapsed > 0.5 {
            format!("{:.0}/s", dirs as f64 / elapsed)
        } else {
            "–".into()
        };

        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .gap(d.spacing.md)
            // Left: scan breakdown
            .child(
                div()
                    .w(px(260.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px(d.spacing.lg)
                            .child(Self::panel_label(d, "BUILD_ARTIFACTS_FOUND")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(d.spacing.sm)
                            .px(d.spacing.lg)
                            .pb(d.spacing.lg)
                            .child(Self::scan_stat_row(
                                d,
                                "ITEMS_FOUND",
                                &items.to_string(),
                                d.colors.accent_green,
                            ))
                            .child(Self::scan_stat_row(
                                d,
                                "DIRS_SCANNED",
                                &format_number(dirs),
                                d.colors.text_secondary,
                            ))
                            .child(Self::scan_stat_row(
                                d,
                                "SIZE_FOUND",
                                &utils::format_size(size_found),
                                d.colors.text_secondary,
                            ))
                            .child(Self::scan_stat_row(
                                d,
                                "ELAPSED",
                                &utils::format_elapsed(elapsed),
                                d.colors.text_tertiary,
                            ))
                            .child(Self::scan_stat_row(
                                d,
                                "SCAN_RATE",
                                &rate,
                                d.colors.text_tertiary,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .p(d.spacing.lg)
                            .child(
                                div()
                                    .text_size(d.typography.size_xxl)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(d.colors.text_primary)
                                    .child(utils::format_size(size_found)),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child("Reclaimable so far"),
                            ),
                    ),
            )
            // Center: progress gauge
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(d.spacing.md)
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .child(Self::render_progress_gauge(d, dirs, items, &current_path))
                    .child(
                        div()
                            .w(px(260.0))
                            .child(ProgressBar::new(d).render_indeterminate()),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_tertiary)
                            .child("Scanning in progress"),
                    ),
            )
            // Right: artifact log
            .child(
                div()
                    .w(px(280.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .bg(d.colors.bg_secondary)
                    .rounded(d.radius.md)
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px(d.spacing.lg)
                            .child(Self::panel_label(d, "ARTIFACT_LOG")),
                    )
                    .child(
                        div()
                            .id("scan-log")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_y_scroll()
                            .px(d.spacing.lg)
                            .pb(d.spacing.md)
                            .gap(px(2.0))
                            .children(scan_log.iter().map(|path| {
                                div()
                                    .font_family("Menlo")
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .overflow_x_hidden()
                                    .child(truncate_path(path, 38))
                            })),
                    ),
            )
    }

    fn render_progress_gauge(
        d: DesignSystem,
        dirs: usize,
        items: usize,
        current_path: &str,
    ) -> Div {
        let _ = current_path;
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(d.spacing.md)
            // Outer ring
            .child(
                div()
                    .w(px(180.0))
                    .h(px(180.0))
                    .rounded_full()
                    .bg(Hsla {
                        a: 0.10,
                        ..d.colors.accent_green
                    })
                    .border_2()
                    .border_color(d.colors.accent_green)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(d.typography.size_title)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(d.colors.text_primary)
                                    .child(format_number(dirs)),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child("Dirs scanned"),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(d.typography.size_md)
                    .text_color(d.colors.accent_green)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{} found", items)),
            )
    }

    fn scan_stat_row(d: DesignSystem, label: &'static str, value: &str, vc: Hsla) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(prettify_label(label)),
            )
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(vc)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value.to_string()),
            )
    }
}

// ---------------------------------------------------------------------------
// File browser panel
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_file_browser(
        d: DesignSystem,
        browse_path: &str,
        entries: &[(String, PathBuf)],
        app: &Entity<ArtifactApp>,
    ) -> Div {
        let app_cancel = app.clone();
        let app_select = app.clone();

        let mut list = div()
            .id("file-browser-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(d.spacing.md)
            .pb(d.spacing.md)
            .gap(px(2.0));

        if entries.is_empty() {
            list = list.child(
                div()
                    .p(d.spacing.md)
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_tertiary)
                    .child("No subdirectories"),
            );
        } else {
            for (name, path) in entries {
                let app_nav = app.clone();
                let nav_path = path.clone();
                let is_parent = name == "..";
                let label = if is_parent {
                    "../".to_string()
                } else {
                    format!("{}/", name)
                };

                list = list.child(
                    div()
                        .id(ElementId::Name(format!("browse-{}", path.display()).into()))
                        .px(d.spacing.md)
                        .py(px(8.0))
                        .rounded(d.radius.sm)
                        .cursor_pointer()
                        .hover(|s| s.bg(d.colors.interactive_hover))
                        .on_click(move |_, _, cx| {
                            app_nav.update(cx, |a, cx| a.browse_navigate(nav_path.clone(), cx));
                        })
                        .child(
                            div()
                                .font_family("Menlo")
                                .text_size(d.typography.size_sm)
                                .text_color(if is_parent {
                                    d.colors.text_tertiary
                                } else {
                                    d.colors.text_secondary
                                })
                                .child(label),
                        ),
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            // Header
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(d.spacing.lg)
                    .h(px(48.0))
                    .child(Self::panel_label(d, "SELECT_DIRECTORY"))
                    .child(
                        div()
                            .font_family("Menlo")
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_tertiary)
                            .child(truncate_path(browse_path, 40)),
                    ),
            )
            .child(list)
            // Actions
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p(d.spacing.lg)
                    .child(
                        Button::new("Cancel", d)
                            .variant(ButtonVariant::Ghost)
                            .render("btn-browse-cancel", move |_, _, cx| {
                                app_cancel.update(cx, |a, cx| a.close_file_browser(cx));
                            }),
                    )
                    .child(Button::new("Select", d).render(
                        "btn-browse-select",
                        move |_, _, cx| {
                            app_select.update(cx, |a, cx| a.browse_select(cx));
                        },
                    )),
            )
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn panel_label(d: DesignSystem, text: &'static str) -> Div {
        let pretty = prettify_label(text);
        div()
            .text_size(d.typography.size_xs)
            .text_color(d.colors.text_tertiary)
            .font_weight(FontWeight::MEDIUM)
            .child(pretty)
    }
}

fn prettify_label(text: &str) -> String {
    let lower = text.to_lowercase().replace('_', " ");
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn truncate_path(path: &str, max: usize) -> String {
    if path.len() <= max {
        path.to_string()
    } else {
        let start = path.len() - (max - 3);
        format!("...{}", &path[start..])
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
