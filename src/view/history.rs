//! History screen: past cleanup runs and aggregate summary.

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::ArtifactView;
use super::widgets::{alpha, format_unix_time};
use crate::app::HistoryRun;
use artifact::theme::{DesignSystem, Gradients};
use artifact::utils::{self, format_number};

impl ArtifactView {
    pub(super) fn render_history_view(&self, d: DesignSystem, cx: &mut Context<Self>) -> Div {
        let history_error = self.history_error.clone();
        let is_loading = self.app.read(cx).is_history_loading();
        let runs = self.history_cache.clone();
        let total_runs = runs.len();
        let total_records: usize = runs.iter().map(|r| r.entries.len()).sum();
        let total_bytes: i64 = runs.iter().map(|r| r.total_bytes).sum();

        let list_panel = Self::panel(
            d,
            "Cleanup_History",
            &format!("{} Runs", format_number(total_runs)),
            self.render_history_list(d, &runs, is_loading, cx),
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
                    !is_loading,
                    false,
                    cx.listener(|this, _, _, cx| {
                        this.refresh_history(cx);
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

    pub(super) fn render_history_list(
        &self,
        d: DesignSystem,
        runs: &[HistoryRun],
        is_loading: bool,
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

        if is_loading && runs.is_empty() {
            list = list.child(
                div()
                    .py(px(16.0))
                    .child(Self::loading_indicator(d, "Loading history…")),
            );
        } else if runs.is_empty() {
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
}
