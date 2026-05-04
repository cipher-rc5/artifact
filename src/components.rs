use crate::theme::DesignSystem;
use gpui::prelude::FluentBuilder;
use gpui::*;

// ---------------------------------------------------------------------------
// Panel (replaces BentoCard)
// ---------------------------------------------------------------------------

pub struct BentoCard {
    design: DesignSystem,
}

impl BentoCard {
    pub fn new(design: DesignSystem) -> Self {
        Self { design }
    }

    pub fn render(&self, content: impl FnOnce(Div) -> Div) -> Div {
        let inner = div()
            .bg(self.design.colors.bg_secondary)
            .border_1()
            .border_color(self.design.colors.border_secondary)
            .rounded(self.design.radius.md)
            .p(self.design.spacing.lg);
        content(inner)
    }
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

pub struct Button {
    label: String,
    variant: ButtonVariant,
    disabled: bool,
    design: DesignSystem,
}

impl Button {
    pub fn new(label: impl Into<String>, design: DesignSystem) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::Primary,
            disabled: false,
            design,
        }
    }

    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn render(
        &self,
        id: impl Into<ElementId>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let d = self.design;
        let disabled = self.disabled;

        let (bg, border_color, text_color) = match (self.variant, disabled) {
            (_, true) => (
                d.colors.bg_tertiary,
                d.colors.interactive_disabled,
                d.colors.text_tertiary,
            ),
            (ButtonVariant::Primary, _) => (
                Hsla {
                    a: 0.92,
                    ..d.colors.accent_green
                },
                d.colors.accent_green,
                d.colors.text_inverse,
            ),
            (ButtonVariant::Danger, _) => (
                Hsla {
                    a: 0.14,
                    ..d.colors.accent_red
                },
                Hsla {
                    a: 0.55,
                    ..d.colors.accent_red
                },
                d.colors.accent_red,
            ),
            (ButtonVariant::Secondary, _) => (
                d.colors.bg_tertiary,
                d.colors.border_primary,
                d.colors.text_primary,
            ),
            (ButtonVariant::Ghost, _) => (
                Hsla {
                    a: 0.0,
                    ..d.colors.bg_secondary
                },
                Hsla {
                    a: 0.0,
                    ..d.colors.border_secondary
                },
                d.colors.text_secondary,
            ),
        };

        div()
            .id(id)
            .px(d.spacing.md)
            .py(px(8.0))
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .rounded_full()
            .when(!disabled, |el| {
                el.hover(|s| s.bg(d.colors.interactive_hover))
                    .active(|s| s.bg(d.colors.interactive_active))
                    .cursor_pointer()
                    .on_click(move |ev, win, cx| on_click(ev, win, cx))
            })
            .child(
                div()
                    .text_color(text_color)
                    .text_size(d.typography.size_sm)
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.label.clone()),
            )
    }
}

// ---------------------------------------------------------------------------
// Badge
// ---------------------------------------------------------------------------

pub struct Badge {
    label: String,
    color: Hsla,
    design: DesignSystem,
}

impl Badge {
    pub fn new(label: impl Into<String>, color: Hsla, design: DesignSystem) -> Self {
        Self {
            label: label.into(),
            color,
            design,
        }
    }

    pub fn render(&self) -> Div {
        div()
            .px(self.design.spacing.sm)
            .py(px(3.0))
            .bg(Hsla {
                a: 0.12,
                ..self.color
            })
            .rounded_full()
            .child(
                div()
                    .text_color(Hsla {
                        a: 0.95,
                        ..self.color
                    })
                    .text_size(self.design.typography.size_xs)
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.label.clone()),
            )
    }
}

// ---------------------------------------------------------------------------
// StatBox
// ---------------------------------------------------------------------------

pub struct StatBox {
    label: String,
    value: String,
    design: DesignSystem,
}

impl StatBox {
    pub fn new(label: impl Into<String>, value: impl Into<String>, design: DesignSystem) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            design,
        }
    }

    pub fn render(&self) -> Div {
        let d = self.design;
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .flex_1()
            .child(
                div()
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_xs)
                    .child(self.label.clone()),
            )
            .child(
                div()
                    .text_color(d.colors.text_primary)
                    .text_size(d.typography.size_xl)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.value.clone()),
            )
    }
}

// ---------------------------------------------------------------------------
// Checkbox — rendered as a pill toggle
// ---------------------------------------------------------------------------

pub struct Checkbox {
    label: String,
    checked: bool,
    design: DesignSystem,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: bool, design: DesignSystem) -> Self {
        Self {
            label: label.into(),
            checked,
            design,
        }
    }

    pub fn render(
        &self,
        id: impl Into<ElementId>,
        on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let d = self.design;
        let checked = self.checked;
        let green = d.colors.accent_green;

        div()
            .id(id)
            .flex()
            .items_center()
            .gap(d.spacing.sm)
            .cursor_pointer()
            .on_click(on_toggle)
            // Pill track
            .child(
                div()
                    .w(px(34.0))
                    .h(px(20.0))
                    .rounded_full()
                    .bg(if checked { green } else { d.colors.bg_tertiary })
                    .flex()
                    .items_center()
                    .px(px(2.0))
                    .child(
                        div()
                            .w(px(14.0))
                            .h(px(14.0))
                            .rounded_full()
                            .flex_shrink_0()
                            .bg(if checked {
                                d.colors.bg_primary
                            } else {
                                d.colors.text_tertiary
                            })
                            .when(checked, |el| el.ml(px(14.0))),
                    ),
            )
            .child(
                div()
                    .text_color(if checked {
                        d.colors.text_primary
                    } else {
                        d.colors.text_secondary
                    })
                    .text_size(d.typography.size_sm)
                    .child(self.label.clone()),
            )
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

pub struct Input {
    placeholder: String,
    value: String,
    design: DesignSystem,
}

impl Input {
    pub fn new(
        placeholder: impl Into<String>,
        value: impl Into<String>,
        design: DesignSystem,
    ) -> Self {
        Self {
            placeholder: placeholder.into(),
            value: value.into(),
            design,
        }
    }

    pub fn render(&self) -> Div {
        let d = self.design;
        div()
            .bg(d.colors.bg_tertiary)
            .rounded_full()
            .px(d.spacing.md)
            .py(px(8.0))
            .child(
                div()
                    .font_family("Menlo")
                    .text_color(if self.value.is_empty() {
                        d.colors.text_tertiary
                    } else {
                        d.colors.text_primary
                    })
                    .text_size(d.typography.size_sm)
                    .child(if self.value.is_empty() {
                        self.placeholder.clone()
                    } else {
                        self.value.clone()
                    }),
            )
    }
}

// ---------------------------------------------------------------------------
// Separator
// ---------------------------------------------------------------------------

pub struct Separator {
    design: DesignSystem,
}

impl Separator {
    pub fn new(design: DesignSystem) -> Self {
        Self { design }
    }

    pub fn render(&self) -> Div {
        div()
            .h(px(1.0))
            .w_full()
            .bg(self.design.colors.border_secondary)
    }
}

// ---------------------------------------------------------------------------
// ProgressBar — segmented block style
// ---------------------------------------------------------------------------

pub struct ProgressBar {
    design: DesignSystem,
}

impl ProgressBar {
    pub fn new(design: DesignSystem) -> Self {
        Self { design }
    }

    pub fn render_indeterminate(&self) -> Div {
        let d = self.design;
        div()
            .w_full()
            .h(px(6.0))
            .rounded_full()
            .bg(d.colors.bg_tertiary)
            .flex()
            .items_center()
            .child(
                div()
                    .w(px(120.0))
                    .h_full()
                    .rounded_full()
                    .bg(d.colors.accent_green),
            )
    }

    pub fn render_progress(&self, progress: f32) -> Div {
        let d = self.design;
        let filled = progress.clamp(0.0, 1.0);
        div()
            .w_full()
            .h(px(6.0))
            .rounded_full()
            .bg(d.colors.bg_tertiary)
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .rounded_full()
                    .bg(d.colors.accent_green)
                    .w(relative(filled)),
            )
    }
}

// ---------------------------------------------------------------------------
// size_blocks — 5-square relative-size indicator (free function)
// ---------------------------------------------------------------------------

pub fn size_blocks(filled: usize, color: Hsla, _design: DesignSystem) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(3.0))
        .children((0..5usize).map(|i| {
            div()
                .w(px(5.0))
                .h(px(14.0))
                .rounded_full()
                .bg(if i < filled {
                    Hsla { a: 0.95, ..color }
                } else {
                    Hsla { a: 0.15, ..color }
                })
        }))
}
