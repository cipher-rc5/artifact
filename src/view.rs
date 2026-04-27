use gpui::prelude::FluentBuilder;
use gpui::*;
use std::path::PathBuf;
use std::time::Duration;

use crate::app::{ArtifactApp, ScanState};
use artifact::components::*;
use artifact::directory_item::DirectoryType;
use artifact::theme::{DesignSystem, Gradients};
use artifact::utils;

pub struct ArtifactView {
    app: Entity<ArtifactApp>,
    design: DesignSystem,
}

impl ArtifactView {
    pub fn new(app: Entity<ArtifactApp>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_view, _entity, cx| cx.notify()).detach();

        let app_clone = app.clone();
        cx.spawn(async move |_this, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                cx.update(|cx| {
                    app_clone.update(cx, |app, cx| app.check_scan_progress(cx));
                });
            }
        })
        .detach();

        Self { app, design: DesignSystem::new() }
    }
}

// ---------------------------------------------------------------------------
// Root render
// ---------------------------------------------------------------------------

impl Render for ArtifactView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let d = self.design;

        let scan_state    = app.scan_state();
        let is_scanning   = scan_state == ScanState::Scanning;
        let scan_path     = app.scan_path().to_string();
        let total_size    = app.total_size();
        let selected_size = app.selected_size();
        let deleted_count = app.deleted_count();
        let error_msg     = app.error_message().map(|s| s.to_string());
        let scan_nm       = app.scan_node_modules();
        let scan_rt       = app.scan_rust_target();
        let show_orphaned = app.show_orphaned_only();
        let progress      = app.scan_progress_data().cloned();
        let file_browser_open = app.is_file_browser_open();
        let browse_path   = app.browse_path().display().to_string();
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
        let has_entries   = !dir_entries.is_empty();
        let visible_count = dir_entries.len();
        let total_count   = app.visible_entries().len();
        let max_bytes: u64 = dir_entries.iter().map(|e| e.4).max().unwrap_or(1).max(1);
        let _ = app;

        // Cloned handles for closures
        let app_scan       = self.app.clone();
        let app_sel_all    = self.app.clone();
        let app_sel_none   = self.app.clone();
        let app_delete     = self.app.clone();
        let app_nm         = self.app.clone();
        let app_rt         = self.app.clone();
        let app_orph       = self.app.clone();
        let app_browse_open = self.app.clone();

        // Root: monospace font applied globally, flex row (sidebar | content)
        div()
            .size_full()
            .font_family("Menlo")
            .bg(d.colors.bg_primary)
            .text_color(d.colors.text_primary)
            .flex()
            .flex_row()
            // Sidebar
            .child(Self::render_sidebar(d))
            // Content area: topbar + panels
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    // Topbar
                    .child(Self::render_topbar(d, scan_state, total_size, visible_count))
                    // Body panels
                    .child(if is_scanning {
                        Self::render_scan_view(d, progress.as_ref(), &scan_log)
                    } else {
                        Self::render_idle_view(
                            d,
                            scan_state,
                            &scan_path,
                            scan_nm,
                            scan_rt,
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
                            app_nm,
                            app_rt,
                            app_orph,
                            app_browse_open,
                        )
                    }),
            )
    }
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn render_sidebar(d: DesignSystem) -> Div {
        div()
            .w(px(40.0))
            .flex_shrink_0()
            .h_full()
            .bg(d.colors.bg_secondary)
            .border_r_1()
            .border_color(d.colors.border_primary)
            .flex()
            .flex_col()
            .items_center()
            // Logo mark
            .child(
                div()
                    .w(px(40.0))
                    .h(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_b_1()
                    .border_color(d.colors.border_primary)
                    .child(
                        div()
                            .w(px(22.0))
                            .h(px(22.0))
                            .rounded_full()
                            .border_1()
                            .border_color(d.colors.accent_green)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .text_color(d.colors.accent_green)
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("A"),
                            ),
                    ),
            )
            // Nav icons
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.0))
                    .pt(d.spacing.sm)
                    .child(Self::sidebar_icon(d, "⊙", true))
                    .child(Self::sidebar_icon(d, "≡", false))
                    .child(Self::sidebar_icon(d, "◈", false)),
            )
            // Settings at bottom
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .pb(d.spacing.sm)
                    .child(Self::sidebar_icon(d, "⊛", false)),
            )
    }

    fn sidebar_icon(d: DesignSystem, icon: &'static str, active: bool) -> Div {
        div()
            .w(px(32.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(d.radius.sm)
            .text_color(if active { d.colors.accent_green } else { d.colors.text_tertiary })
            .text_size(px(14.0))
            .hover(|s| s.text_color(d.colors.text_secondary).bg(d.colors.interactive_hover))
            .cursor_pointer()
            .child(icon)
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
    ) -> Div {
        let status_label = match scan_state {
            ScanState::Idle     => "IDLE",
            ScanState::Scanning => "SCANNING",
            ScanState::Complete => "COMPLETE",
        };
        let status_color = match scan_state {
            ScanState::Idle     => d.colors.text_tertiary,
            ScanState::Scanning => d.colors.accent_orange,
            ScanState::Complete => d.colors.accent_green,
        };

        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            // Brand row
            .child(
                div()
                    .h(px(40.0))
                    .flex()
                    .items_center()
                    .px(d.spacing.md)
                    .gap(d.spacing.lg)
                    .border_b_1()
                    .border_color(d.colors.border_primary)
                    // Brand
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(d.spacing.sm)
                            .child(
                                div()
                                    .text_size(d.typography.size_title)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(d.colors.text_primary)
                                    .child("ARTIFACT"),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.accent_green)
                                    .child("GPUI CLEANER v0.1.0"),
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
                            .child(Self::topbar_stat(d, "SCAN_STATUS", status_label, status_color))
                            .child(Self::topbar_stat(
                                d,
                                "ITEMS_FOUND",
                                &item_count.to_string(),
                                d.colors.text_secondary,
                            ))
                            .child(Self::topbar_stat(
                                d,
                                "TOTAL_SIZE",
                                &utils::format_size(total_size),
                                d.colors.text_secondary,
                            )),
                    ),
            )
            // Green accent line
            .child(div().h(px(1.0)).w_full().bg(Gradients::header()))
    }

    fn topbar_stat(d: DesignSystem, label: &'static str, value: &str, value_color: Hsla) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(label),
            )
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(value_color)
                    .font_weight(FontWeight::BOLD)
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
        scan_nm: bool,
        scan_rt: bool,
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
        app_nm: Entity<ArtifactApp>,
        app_rt: Entity<ArtifactApp>,
        app_orph: Entity<ArtifactApp>,
        app_browse_open: Entity<ArtifactApp>,
    ) -> Div {
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            // ── Left panel (scan config + artifact list) ──────────────────
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    // SCAN_CONFIG section
                    .child(
                        div()
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(d.colors.border_primary)
                            .p(d.spacing.md)
                            .flex()
                            .flex_col()
                            .gap(d.spacing.sm)
                            .child(Self::panel_label(d, "SCAN_CONFIG"))
                            // Path + browse
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(d.spacing.sm)
                                    .child(div().flex_1().child(Input::new("SCAN_PATH...", scan_path, d).render()))
                                    .child(
                                        Button::new("Browse", d)
                                            .variant(ButtonVariant::Secondary)
                                            .render("btn-browse", move |_, _, cx| {
                                                app_browse_open.update(cx, |a, cx| a.open_file_browser(cx));
                                            }),
                                    ),
                            )
                            // Toggles row
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(d.spacing.lg)
                                    .child(Checkbox::new("node_modules", scan_nm, d).render(
                                        "cb-nm",
                                        move |_, _, cx| {
                                            app_nm.update(cx, |a, cx| a.toggle_node_modules(cx));
                                        },
                                    ))
                                    .child(Checkbox::new("rust target", scan_rt, d).render(
                                        "cb-rt",
                                        move |_, _, cx| {
                                            app_rt.update(cx, |a, cx| a.toggle_rust_target(cx));
                                        },
                                    ))
                                    .child(Checkbox::new("orphaned only", show_orphaned, d).render(
                                        "cb-orph",
                                        move |_, _, cx| {
                                            app_orph.update(cx, |a, cx| a.toggle_orphaned_only(cx));
                                        },
                                    )),
                            )
                            // Scan button
                            .child(
                                Button::new("Scan", d).render("btn-scan", move |_, _, cx| {
                                    app_scan.update(cx, |a, cx| a.start_scan(cx));
                                }),
                            ),
                    )
                    // File browser (replaces artifact list when open)
                    .when(file_browser_open, |root| {
                        root.child(Self::render_file_browser(d, browse_path, browse_entries, app))
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
                total_size,
                selected_size,
                deleted_count,
                error_msg,
                has_entries,
                app_delete,
            ))
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
                    .px(d.spacing.md)
                    .py(d.spacing.sm)
                    .border_b_1()
                    .border_color(d.colors.border_primary)
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
                                    .child(format!("{} OF {}", visible_count, total_count)),
                            ),
                    )
                    .when(has_entries, |row| {
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    Button::new("All", d)
                                        .variant(ButtonVariant::Ghost)
                                        .render("btn-sel-all", move |_, _, cx| {
                                            app_sel_all.update(cx, |a, cx| a.select_all(cx));
                                        }),
                                )
                                .child(
                                    Button::new("None", d)
                                        .variant(ButtonVariant::Ghost)
                                        .render("btn-sel-none", move |_, _, cx| {
                                            app_sel_none.update(cx, |a, cx| a.select_none(cx));
                                        }),
                                ),
                        )
                    }),
            )
            // Scrollable list
            .child(Self::render_dir_rows(d, scan_state, entries, max_bytes, app))
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
            .overflow_y_scroll();

        if entries.is_empty() {
            list = list.child(
                div()
                    .p(d.spacing.md)
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_sm)
                    .child(match scan_state {
                        ScanState::Idle     => "RUN_SCAN TO FIND BUILD ARTIFACTS",
                        ScanState::Scanning => "SCANNING...",
                        ScanState::Complete => "NO_ARTIFACTS FOUND",
                    }),
            );
        } else {
            for (idx, path, dir_type, project_name, size_bytes, selected, is_orphaned) in entries {
                let app_toggle = app.clone();
                let idx = *idx;
                let size_bytes = *size_bytes;
                let selected = *selected;
                let is_orphaned = *is_orphaned;

                let badge_color = match dir_type {
                    DirectoryType::NodeModules => d.colors.accent_green,
                    DirectoryType::RustTarget  => d.colors.accent_orange,
                };
                let badge_label = dir_type.to_string();
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
                        .py(px(6.0))
                        .gap(d.spacing.sm)
                        .border_b_1()
                        .border_color(d.colors.border_secondary)
                        .cursor_pointer()
                        .when(selected, |el| el.bg(Hsla { a: 0.08, ..d.colors.accent_green }))
                        .hover(|s| s.bg(d.colors.interactive_hover))
                        .on_click(move |_, _, cx| {
                            app_toggle.update(cx, |a, cx| a.toggle_selection(idx, cx));
                        })
                        // Selected indicator bar
                        .child(
                            div()
                                .w(px(2.0))
                                .h(px(28.0))
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
                                                    .child("ORPHANED"),
                                            )
                                        }),
                                ),
                        )
                        // Size value
                        .child(
                            div()
                                .text_size(d.typography.size_sm)
                                .text_color(badge_color)
                                .font_weight(FontWeight::BOLD)
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
        total_size: u64,
        selected_size: u64,
        deleted_count: usize,
        error_msg: Option<&str>,
        has_entries: bool,
        app_delete: Entity<ArtifactApp>,
    ) -> Div {
        div()
            .w(px(240.0))
            .flex_shrink_0()
            .h_full()
            .bg(d.colors.bg_secondary)
            .border_l_1()
            .border_color(d.colors.border_primary)
            .flex()
            .flex_col()
            // Header
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .px(d.spacing.md)
                    .border_b_1()
                    .border_color(d.colors.border_primary)
                    .child(Self::panel_label(d, "SYSTEM_RESULTS")),
            )
            // Total reclaimable (hero stat)
            .child(
                div()
                    .flex_shrink_0()
                    .p(d.spacing.md)
                    .border_b_1()
                    .border_color(d.colors.border_primary)
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_tertiary)
                            .child("TOTAL_RECLAIMABLE"),
                    )
                    .child(
                        div()
                            .text_size(px(32.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(d.colors.text_primary)
                            .child(utils::format_size(total_size)),
                    ),
            )
            // Stats rows
            .child(
                div()
                    .flex_shrink_0()
                    .p(d.spacing.md)
                    .border_b_1()
                    .border_color(d.colors.border_primary)
                    .flex()
                    .flex_col()
                    .gap(d.spacing.sm)
                    .child(Self::stat_row(d, "SELECTED_SIZE", &utils::format_size(selected_size), d.colors.accent_green))
                    .child(Self::stat_row(d, "ITEMS_DELETED", &deleted_count.to_string(), d.colors.text_secondary))
                    .child(Self::stat_row(d, "ITEMS_FOUND", &format!("{}", if total_size > 0 { "–" } else { "0" }), d.colors.text_tertiary)),
            )
            // Status indicator
            .child(
                div()
                    .flex_shrink_0()
                    .px(d.spacing.md)
                    .py(d.spacing.sm)
                    .border_b_1()
                    .border_color(d.colors.border_primary)
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded_full()
                            .bg(d.colors.accent_green),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.accent_green)
                            .child("LINK_DIRECT"),
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
                        .mx(d.spacing.md)
                        .mb(d.spacing.sm)
                        .p(d.spacing.sm)
                        .border_1()
                        .border_color(d.colors.accent_orange)
                        .rounded(d.radius.sm)
                        .bg(Hsla { a: 0.08, ..d.colors.accent_orange })
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
                div()
                    .flex_shrink_0()
                    .p(d.spacing.md)
                    .child(
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
                    .child(label),
            )
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(value_color)
                    .font_weight(FontWeight::BOLD)
                    .child(value.to_string()),
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
            // Left: scan breakdown
            .child(
                div()
                    .w(px(220.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(d.colors.border_primary)
                    .child(
                        div()
                            .h(px(36.0))
                            .flex()
                            .items_center()
                            .px(d.spacing.md)
                            .border_b_1()
                            .border_color(d.colors.border_primary)
                            .child(Self::panel_label(d, "BUILD_ARTIFACTS_FOUND")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(d.spacing.sm)
                            .p(d.spacing.md)
                            .child(Self::scan_stat_row(d, "ITEMS_FOUND", &items.to_string(), d.colors.accent_green))
                            .child(Self::scan_stat_row(d, "DIRS_SCANNED", &format_number(dirs), d.colors.text_secondary))
                            .child(Self::scan_stat_row(d, "SIZE_FOUND", &utils::format_size(size_found), d.colors.text_secondary))
                            .child(Self::scan_stat_row(d, "ELAPSED", &utils::format_elapsed(elapsed), d.colors.text_tertiary))
                            .child(Self::scan_stat_row(d, "SCAN_RATE", &rate, d.colors.text_tertiary)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .p(d.spacing.md)
                            .child(
                                div()
                                    .text_size(px(28.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(d.colors.text_primary)
                                    .child(utils::format_size(size_found)),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child("RECLAIMABLE_SO_FAR"),
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
                    .child(Self::render_progress_gauge(d, dirs, items, &current_path))
                    // Segmented progress bar
                    .child(
                        div()
                            .w(px(260.0))
                            .child(ProgressBar::new(d).render_indeterminate()),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_tertiary)
                            .child("SCANNING_IN_PROGRESS"),
                    ),
            )
            // Right: artifact log
            .child(
                div()
                    .w(px(260.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .border_l_1()
                    .border_color(d.colors.border_primary)
                    .child(
                        div()
                            .h(px(36.0))
                            .flex()
                            .items_center()
                            .px(d.spacing.md)
                            .border_b_1()
                            .border_color(d.colors.border_primary)
                            .child(Self::panel_label(d, "ARTIFACT_LOG")),
                    )
                    .child(
                        div()
                            .id("scan-log")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_y_scroll()
                            .p(d.spacing.sm)
                            .gap(px(1.0))
                            .children(scan_log.iter().map(|path| {
                                div()
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
            .gap(d.spacing.sm)
            // Outer ring
            .child(
                div()
                    .w(px(140.0))
                    .h(px(140.0))
                    .rounded_full()
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
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(36.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(d.colors.text_primary)
                                    .child(format_number(dirs)),
                            )
                            .child(
                                div()
                                    .text_size(d.typography.size_xs)
                                    .text_color(d.colors.text_tertiary)
                                    .child("DIRS_SCANNED"),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.accent_green)
                    .child(format!("{} FOUND", items)),
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
                    .child(label),
            )
            .child(
                div()
                    .text_size(d.typography.size_sm)
                    .text_color(vc)
                    .font_weight(FontWeight::BOLD)
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
            .bg(d.colors.bg_primary);

        if entries.is_empty() {
            list = list.child(
                div()
                    .p(d.spacing.md)
                    .text_size(d.typography.size_sm)
                    .text_color(d.colors.text_tertiary)
                    .child("NO_SUBDIRECTORIES"),
            );
        } else {
            for (name, path) in entries {
                let app_nav  = app.clone();
                let nav_path = path.clone();
                let is_parent = name == "..";
                let label = if is_parent { "../".to_string() } else { format!("{}/", name) };

                list = list.child(
                    div()
                        .id(ElementId::Name(format!("browse-{}", path.display()).into()))
                        .px(d.spacing.md)
                        .py(px(5.0))
                        .border_b_1()
                        .border_color(d.colors.border_secondary)
                        .cursor_pointer()
                        .hover(|s| s.bg(d.colors.interactive_hover))
                        .on_click(move |_, _, cx| {
                            app_nav.update(cx, |a, cx| a.browse_navigate(nav_path.clone(), cx));
                        })
                        .child(
                            div()
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
                    .px(d.spacing.md)
                    .h(px(36.0))
                    .border_b_1()
                    .border_color(d.colors.border_primary)
                    .child(Self::panel_label(d, "SELECT_DIRECTORY"))
                    .child(
                        div()
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
                    .p(d.spacing.md)
                    .border_t_1()
                    .border_color(d.colors.border_primary)
                    .child(
                        Button::new("Cancel", d)
                            .variant(ButtonVariant::Ghost)
                            .render("btn-browse-cancel", move |_, _, cx| {
                                app_cancel.update(cx, |a, cx| a.close_file_browser(cx));
                            }),
                    )
                    .child(Button::new("Select", d).render("btn-browse-select", move |_, _, cx| {
                        app_select.update(cx, |a, cx| a.browse_select(cx));
                    })),
            )
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

impl ArtifactView {
    fn panel_label(d: DesignSystem, text: &'static str) -> Div {
        div()
            .text_size(d.typography.size_xs)
            .text_color(d.colors.text_tertiary)
            .font_weight(FontWeight::BOLD)
            .child(text)
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
