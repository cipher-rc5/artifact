//! Shared visual primitives and pure helper functions for the view layer.
//!
//! The widget builders live in an `impl ArtifactView` block so existing call
//! sites keep using `Self::terminal_button(..)` etc. The free functions are the
//! `cx`-free helpers used across the screen modules.

use gpui::prelude::FluentBuilder;
use gpui::*;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{ArtifactBucket, ArtifactView, LanguageSetting, SidebarIcon, ViewEntry};
use artifact::directory_item::DirectoryType;
use artifact::rules::{self, ColorHint};
use artifact::theme::{DesignSystem, Gradients};

pub(super) fn rule_color(d: DesignSystem, hint: ColorHint) -> Hsla {
    match hint {
        ColorHint::Green => d.colors.accent_green,
        ColorHint::Orange => d.colors.accent_orange,
        ColorHint::Blue => d.colors.accent_blue,
        ColorHint::Yellow => d.colors.accent_yellow,
        ColorHint::Purple => d.colors.accent_purple,
        ColorHint::Red => d.colors.accent_red,
    }
}

pub(super) fn alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

impl ArtifactView {
    pub(super) fn app_mark(d: DesignSystem) -> Div {
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

    pub(super) fn scroll_overlay(d: DesignSystem, handle: &ScrollHandle) -> Div {
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

    pub(super) fn panel(d: DesignSystem, title: &'static str, meta: &str, body: Div) -> Div {
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

    pub(super) fn panel_label(d: DesignSystem, text: &'static str) -> Div {
        div()
            .text_size(d.typography.size_sm)
            .text_color(d.colors.text_secondary)
            .font_weight(FontWeight::SEMIBOLD)
            .child(text)
    }

    pub(super) fn separator(d: DesignSystem) -> Div {
        div().h(px(1.0)).w_full().bg(d.colors.border_secondary)
    }

    /// Small inline spinner-style loading indicator (review finding H4). Rendered
    /// as a labelled pip so it reads honestly while background I/O is in flight.
    pub(super) fn loading_indicator(d: DesignSystem, label: &str) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded_full()
                    .bg(d.colors.accent_orange),
            )
            .child(
                div()
                    .text_size(d.typography.size_xs)
                    .text_color(d.colors.text_secondary)
                    .child(label.to_string()),
            )
    }

    pub(super) fn meter_bar(
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

    pub(super) fn results_metric_line(d: DesignSystem, label: &str, value: &str) -> Div {
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

    pub(super) fn toggle_row(
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

    pub(super) fn action_toggle(
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

    pub(super) fn terminal_button(
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

    pub(super) fn terminal_button_sm(
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
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

pub(super) fn format_unix_time(ts: i64) -> String {
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

pub(super) fn sidebar_icon_name(icon: SidebarIcon) -> &'static str {
    match icon {
        SidebarIcon::Dashboard => "dashboard",
        SidebarIcon::Results => "results",
        SidebarIcon::Browser => "browser",
        SidebarIcon::History => "history",
        SidebarIcon::Settings => "settings",
    }
}

pub(super) fn summarize_languages(
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

pub(super) fn summarize_artifacts(entries: &[ViewEntry]) -> Vec<ArtifactBucket> {
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

pub(super) fn summary_windows(buckets: &[ArtifactBucket]) -> Vec<ArtifactBucket> {
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

pub(super) fn scaled_segments(
    bucket_size: u64,
    buckets: &[ArtifactBucket],
    max_segments: usize,
) -> usize {
    let max = buckets
        .iter()
        .map(|bucket| bucket.size_bytes)
        .max()
        .unwrap_or(1);
    scaled_segments_from_max(bucket_size, max, max_segments)
}

pub(super) fn scaled_segments_from_max(size: u64, max: u64, max_segments: usize) -> usize {
    if size == 0 || max == 0 {
        1
    } else {
        (((size as f32 / max as f32) * max_segments as f32).ceil() as usize).clamp(1, max_segments)
    }
}

pub(super) fn entry_type_label(dir_type: DirectoryType) -> &'static str {
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

/// Truncate `text` to at most `max` characters (not bytes), appending an
/// ellipsis when shortened. Operates on `char` boundaries so multi-byte UTF-8
/// paths (CJK, emoji, accented folder names) never panic (review finding C3).
pub(super) fn truncate_end(text: &str, max: usize) -> String {
    // Count characters without materialising the whole iterator when the string
    // is already short.
    if text.chars().count() <= max {
        return text.to_string();
    }

    let keep = max.saturating_sub(3);
    let truncated: String = text.chars().take(keep).collect();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::truncate_end;

    #[test]
    fn truncate_end_ascii_shorter_than_max_is_unchanged() {
        assert_eq!(truncate_end("short", 10), "short");
    }

    #[test]
    fn truncate_end_ascii_truncates_with_ellipsis() {
        assert_eq!(truncate_end("abcdefghij", 6), "abc...");
    }

    #[test]
    fn truncate_end_multibyte_does_not_panic() {
        // CJK: each character is 3 bytes; byte-slicing would panic mid-codepoint.
        let cjk = "路径/项目/构建输出目录名称很长";
        let out = truncate_end(cjk, 6);
        assert!(out.ends_with("..."));
        // 3 kept chars + "..." => 6 chars total.
        assert_eq!(out.chars().count(), 6);
    }

    #[test]
    fn truncate_end_emoji_does_not_panic() {
        // Emoji are 4-byte code points; boundary would land mid-codepoint.
        let emoji = "📁📂🗂️🧱🧰🔧🪛🧲🗑️📦";
        let out = truncate_end(emoji, 5);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn truncate_end_accented_path() {
        let accented = "/Users/José/Développement/café/artéfacts";
        let out = truncate_end(accented, 12);
        assert!(out.ends_with("..."));
        assert_eq!(out.chars().count(), 12);
    }
}
