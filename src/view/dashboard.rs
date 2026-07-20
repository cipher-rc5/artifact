//! Dashboard screen: scan gauge, artifact clusters, savings chart, activity log.

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::widgets::{alpha, scaled_segments};
use super::{ArtifactBucket, ArtifactView, ScanState, SidebarView};
use artifact::theme::{DesignSystem, Gradients};
use artifact::utils::{self, format_number};

/// Grouped inputs for [`ArtifactView::render_gauge`], replacing a long
/// positional argument list (review finding H1).
pub(super) struct GaugeState<'a> {
    pub readiness: usize,
    pub status_label: &'a str,
    pub item_count: usize,
    pub dirs_scanned: usize,
    pub elapsed_secs: f64,
    pub progress_path: &'a str,
    pub compact: bool,
    pub is_scanning: bool,
}

impl ArtifactView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_dashboard_view(
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
                            GaugeState {
                                readiness,
                                status_label,
                                item_count,
                                dirs_scanned: progress_dirs,
                                elapsed_secs: progress_elapsed,
                                progress_path: &progress_path,
                                compact: dense,
                                is_scanning: matches!(scan_state, ScanState::Scanning),
                            },
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
                                        .child("Scan mode: full recursive"),
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

    pub(super) fn render_savings_chart(d: DesignSystem, buckets: &[ArtifactBucket]) -> Div {
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

    pub(super) fn render_gauge(d: DesignSystem, g: GaugeState<'_>) -> Div {
        let GaugeState {
            readiness,
            status_label,
            item_count,
            dirs_scanned,
            elapsed_secs,
            progress_path,
            compact,
            is_scanning,
        } = g;
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
            .when(dirs_scanned == 0 && item_count > 0, |el| {
                el.child(
                    div()
                        .text_size(d.typography.size_xs)
                        .text_color(d.colors.text_tertiary)
                        .child(format!("{} Artifacts Tracked", format_number(item_count))),
                )
            })
    }

    pub(super) fn status_callout(d: DesignSystem, label: &str, value: &str, color: Hsla) -> Div {
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

    pub(super) fn health_metric(
        d: DesignSystem,
        label: &str,
        value: &str,
        color: Hsla,
        filled: usize,
    ) -> Div {
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

    pub(super) fn render_activity_log(&self, d: DesignSystem, scan_log: &[String]) -> Div {
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
}
