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
            .border_color(self.design.colors.border_primary)
            .rounded(self.design.radius.sm)
            .p(self.design.spacing.md);
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

        let (border_color, text_color) = match (self.variant, disabled) {
            (_, true) => (d.colors.interactive_disabled, d.colors.text_tertiary),
            (ButtonVariant::Primary, _) => (d.colors.accent_green, d.colors.accent_green),
            (ButtonVariant::Danger, _) => (d.colors.accent_red, d.colors.accent_red),
            (ButtonVariant::Secondary, _) => (d.colors.border_primary, d.colors.text_secondary),
            (ButtonVariant::Ghost, _) => (d.colors.border_secondary, d.colors.text_tertiary),
        };

        div()
            .id(id)
            .px(d.spacing.md)
            .py(px(5.0))
            .bg(d.colors.bg_secondary)
            .border_1()
            .border_color(border_color)
            .rounded(d.radius.xs)
            .when(!disabled, |el| {
                el.hover(|s| s.bg(d.colors.interactive_hover))
                    .active(|s| s.bg(d.colors.interactive_active))
                    .cursor_pointer()
                    .on_click(move |ev, win, cx| on_click(ev, win, cx))
            })
            .child(
                div()
                    .font_family("Menlo")
                    .text_color(text_color)
                    .text_size(d.typography.size_sm)
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.label.to_uppercase()),
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
        Self { label: label.into(), color, design }
    }

    pub fn render(&self) -> Div {
        div()
            .px(px(5.0))
            .py(px(2.0))
            .border_1()
            .border_color(Hsla { a: 0.50, ..self.color })
            .rounded(self.design.radius.xs)
            .child(
                div()
                    .font_family("Menlo")
                    .text_color(Hsla { a: 0.85, ..self.color })
                    .text_size(self.design.typography.size_xs)
                    .child(self.label.to_uppercase()),
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
    pub fn new(
        label: impl Into<String>,
        value: impl Into<String>,
        design: DesignSystem,
    ) -> Self {
        Self { label: label.into(), value: value.into(), design }
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
                    .font_family("Menlo")
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_xs)
                    .child(self.label.to_uppercase()),
            )
            .child(
                div()
                    .font_family("Menlo")
                    .text_color(d.colors.text_primary)
                    .text_size(d.typography.size_xl)
                    .font_weight(FontWeight::BOLD)
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
        Self { label: label.into(), checked, design }
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
                    .w(px(30.0))
                    .h(px(15.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if checked { green } else { d.colors.border_primary })
                    .bg(if checked {
                        Hsla { a: 0.18, ..green }
                    } else {
                        d.colors.bg_secondary
                    })
                    .flex()
                    .items_center()
                    .px(px(2.0))
                    // Knob — slides right when checked via left margin
                    .child(
                        div()
                            .w(px(10.0))
                            .h(px(10.0))
                            .rounded_full()
                            .flex_shrink_0()
                            .bg(if checked { green } else { d.colors.text_tertiary })
                            .when(checked, |el| el.ml(px(13.0))),
                    ),
            )
            .child(
                div()
                    .font_family("Menlo")
                    .text_color(if checked { d.colors.text_primary } else { d.colors.text_secondary })
                    .text_size(d.typography.size_sm)
                    .child(self.label.to_uppercase()),
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
        Self { placeholder: placeholder.into(), value: value.into(), design }
    }

    pub fn render(&self) -> Div {
        let d = self.design;
        div()
            .bg(d.colors.bg_primary)
            .border_1()
            .border_color(d.colors.border_primary)
            .rounded(d.radius.xs)
            .px(d.spacing.sm)
            .py(px(5.0))
            .child(
                div()
                    .font_family("Menlo")
                    .text_color(if self.value.is_empty() {
                        d.colors.text_tertiary
                    } else {
                        d.colors.text_secondary
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
        div().h(px(1.0)).w_full().bg(self.design.colors.border_secondary)
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
            .h(px(4.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .children((0..24usize).map(|i| {
                let alpha: f32 = match i % 4 {
                    0 => 0.90,
                    1 => 0.45,
                    2 => 0.20,
                    _ => 0.08,
                };
                div()
                    .w(px(12.0))
                    .h_full()
                    .flex_shrink_0()
                    .bg(Hsla { a: alpha, ..d.colors.accent_green })
            }))
    }
}

// ---------------------------------------------------------------------------
// size_blocks — 5-square relative-size indicator (free function)
// ---------------------------------------------------------------------------

pub fn size_blocks(filled: usize, color: Hsla, design: DesignSystem) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .children((0..5usize).map(|i| {
            div()
                .w(px(6.0))
                .h(px(10.0))
                .rounded(design.radius.xs)
                .bg(if i < filled {
                    Hsla { a: 0.90, ..color }
                } else {
                    Hsla { a: 0.12, ..color }
                })
        }))
}
