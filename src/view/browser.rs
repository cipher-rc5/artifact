//! File browser screen: pick the scan root and tune scan parameters.

use gpui::prelude::FluentBuilder;
use gpui::*;
use std::path::PathBuf;

use super::widgets::truncate_end;
use super::{ArtifactView, ScanState, SidebarView};
use artifact::theme::{DesignSystem, Gradients};

impl ArtifactView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_browser_view(
        &mut self,
        d: DesignSystem,
        scan_state: ScanState,
        scan_path: &str,
        browse_path: &str,
        browse_entries: &[(String, PathBuf)],
        file_browser_open: bool,
        can_browse_back: bool,
        can_browse_forward: bool,
        is_browse_loading: bool,
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
                    is_browse_loading,
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
                            .child("Open the file browser to change the scan root."),
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
            "Control",
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_browser_list(
        &self,
        d: DesignSystem,
        browse_path: &str,
        entries: &[(String, PathBuf)],
        can_browse_back: bool,
        can_browse_forward: bool,
        is_browse_loading: bool,
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

        if is_browse_loading {
            list = list.child(
                div()
                    .py(px(12.0))
                    .child(Self::loading_indicator(d, "Listing directory…")),
            );
        } else if entries.is_empty() {
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
                    )
                    .when(is_browse_loading, |row| {
                        row.child(Self::loading_indicator(d, "Loading…"))
                    }),
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
}
