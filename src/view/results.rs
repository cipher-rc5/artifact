//! Results screen: artifact inventory table and the purge/action sidebar.

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::widgets::{alpha, entry_type_label, rule_color, scaled_segments_from_max, truncate_end};
use super::{ArtifactView, ScanState, ViewEntry};
use crate::app::{ArtifactApp, DeleteError};
use artifact::config::DeleteMode;
use artifact::theme::{DesignSystem, Gradients};
use artifact::utils::{self, format_number};

impl ArtifactView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_results_view(
        &self,
        d: DesignSystem,
        scan_state: ScanState,
        entries: &[ViewEntry],
        total_size: u64,
        selected_size: u64,
        selected_count: usize,
        error_msg: Option<&str>,
        delete_errors: &[DeleteError],
        deleted_count: usize,
        delete_mode: DeleteMode,
        is_deleting: bool,
        can_cancel_delete: bool,
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
            "Purge_Parameters",
            "Action",
            Self::results_sidebar(
                d,
                total_size,
                selected_size,
                entries.len(),
                selected_count,
                scan_state_text,
                error_msg,
                delete_errors,
                deleted_count,
                delete_mode,
                is_deleting,
                can_cancel_delete,
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

    pub(super) fn inventory_header(d: DesignSystem, compact: bool) -> Div {
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
                .child(header("Component_Path").flex_1())
                .child(header("Type").w(px(112.0)).flex_shrink_0())
                .child(header("Size").w(px(72.0)).flex_shrink_0())
                .child(header("Metric").w(px(96.0)).flex_shrink_0())
                .child(header("Action").w(px(36.0)).flex_shrink_0())
        }
    }

    pub(super) fn render_inventory_rows(
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
                let expanded = self.expanded_rows.contains(&entry.path_buf);
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

    pub(super) fn render_inventory_row(
        &self,
        d: DesignSystem,
        entry: &ViewEntry,
        max_bytes: u64,
        compact: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
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

        // Selection is keyed by the item's stable path, not its vector index,
        // so it survives the `retain(..)` that runs mid-delete (M4).
        let toggle_path = entry.path_buf.clone();
        let action = Self::action_toggle(
            d,
            ElementId::Name(format!("toggle-{}", entry.path).into()),
            selected,
            cx.listener(move |this, _, _, cx| {
                let toggle_path = toggle_path.clone();
                this.app
                    .update(cx, |app, cx| app.toggle_selection_by_path(&toggle_path, cx));
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

        // Row-expansion is also keyed by path (M4).
        let click_path = entry.path_buf.clone();
        let mut row = div()
            .id(ElementId::Name(format!("inventory-{}", entry.path).into()))
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
                this.toggle_row_expanded(click_path.clone(), cx);
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
    pub(super) fn results_sidebar(
        d: DesignSystem,
        total_size: u64,
        selected_size: u64,
        artifact_count: usize,
        selected_count: usize,
        risk_level: &str,
        error_msg: Option<&str>,
        delete_errors: &[DeleteError],
        deleted_count: usize,
        delete_mode: DeleteMode,
        is_deleting: bool,
        can_cancel_delete: bool,
        app: Entity<ArtifactApp>,
    ) -> Div {
        let action_enabled = selected_size > 0 && !is_deleting;
        let has_results = artifact_count > 0;
        let app_delete = app.clone();
        let app_cancel = app.clone();
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
            .child(Self::results_metric_line(d, "Scan_State", risk_level))
            .child(Self::separator(d))
            .child(Self::results_metric_line(
                d,
                "Last_cleanup",
                if deleted_count == 0 {
                    "None yet"
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
                        .mb(px(12.0))
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
            .when(!delete_errors.is_empty(), |panel| {
                panel.child(Self::delete_errors_panel(d, delete_errors))
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
                    ))
                    .when(can_cancel_delete, |col| {
                        col.child(Self::terminal_button(
                            d,
                            "cancel-delete-btn",
                            "Cancel Delete",
                            true,
                            false,
                            move |_, _, cx| {
                                app_cancel.update(cx, |app, cx| app.cancel_delete(cx));
                            },
                        ))
                    }),
            )
    }

    /// Render the detailed per-item delete failures (path + reason) rather than
    /// just the count (review finding M7).
    fn delete_errors_panel(d: DesignSystem, delete_errors: &[DeleteError]) -> Div {
        let mut panel = div()
            .mb(px(12.0))
            .p(px(14.0))
            .border_1()
            .border_color(alpha(d.colors.accent_red, 0.55))
            .rounded(d.radius.xs)
            .bg(Gradients::notice_glow(d.colors.accent_red))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(d.colors.accent_red)
                    .child(format!(
                        "{} item{} could not be deleted:",
                        delete_errors.len(),
                        if delete_errors.len() == 1 { "" } else { "s" }
                    )),
            );

        for err in delete_errors.iter().take(12) {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_primary)
                            .child(truncate_end(&err.path, 56)),
                    )
                    .child(
                        div()
                            .text_size(d.typography.size_xs)
                            .text_color(d.colors.text_secondary)
                            .child(err.reason.clone()),
                    ),
            );
        }

        if delete_errors.len() > 12 {
            panel = panel.child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_tertiary)
                    .child(format!("… and {} more", delete_errors.len() - 12)),
            );
        }

        panel
    }
}
