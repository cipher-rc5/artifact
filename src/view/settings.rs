//! Settings screen: language/ruleset toggles and delete-behavior options.

use gpui::*;

use super::widgets::{alpha, truncate_end};
use super::{ArtifactView, LanguageSetting, SidebarView};
use crate::app::ArtifactApp;
use artifact::config::DeleteMode;
use artifact::theme::{DesignSystem, Gradients};

impl ArtifactView {
    pub(super) fn render_settings_view(
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

    pub(super) fn language_settings_list(
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

    pub(super) fn delete_mode_option(
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
}
