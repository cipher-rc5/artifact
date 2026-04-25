// file: src/components.rs
// description: Sleek UI components with gradient accents and sharp edges

use crate::theme::{DesignSystem, Gradients};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Card component - sharp-edged elevated surface
pub struct BentoCard {
    design: DesignSystem,
}

impl BentoCard {
    pub fn new(design: DesignSystem) -> Self {
        Self { design }
    }

    pub fn render(&self, content: impl FnOnce(Div) -> Div) -> Div {
        let inner = div()
            .bg(Gradients::surface(&self.design.colors))
            .border_1()
            .border_color(self.design.colors.border_primary)
            .rounded(self.design.radius.md)
            .p(self.design.spacing.md);
        content(inner)
    }
}

/// Button component with variants
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

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn render(
        &self,
        id: impl Into<ElementId>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let text_color = self.design.colors.text_primary;

        let hover_color = self.design.colors.interactive_hover;
        let active_color = self.design.colors.interactive_active;
        let disabled = self.disabled;

        let mut el = div()
            .id(id)
            .px(self.design.spacing.md)
            .py(self.design.spacing.sm)
            .rounded(self.design.radius.sm)
            .border_1()
            .border_color(self.design.colors.border_primary);

        // Apply background based on variant
        el = match (self.variant, self.disabled) {
            (_, true) => el.bg(self.design.colors.interactive_disabled),
            (ButtonVariant::Primary, _) => el
                .bg(Gradients::blue_purple(&self.design.colors))
                .border_color(self.design.colors.accent_blue),
            (ButtonVariant::Danger, _) => el
                .bg(Gradients::red_orange(&self.design.colors))
                .border_color(self.design.colors.accent_red),
            (ButtonVariant::Secondary, _) => el.bg(self.design.colors.bg_tertiary),
            (ButtonVariant::Ghost, _) => el
                .bg(self.design.colors.bg_secondary)
                .border_color(self.design.colors.border_secondary),
        };

        el.when(!disabled, |d| {
            d.hover(|style| style.bg(hover_color))
                .active(|style| style.bg(active_color))
                .cursor_pointer()
                .on_click(move |event, window, cx| on_click(event, window, cx))
        })
        .child(
            div()
                .text_color(text_color)
                .text_size(self.design.typography.size_sm)
                .font_weight(FontWeight::MEDIUM)
                .child(self.label.clone()),
        )
    }
}

/// Badge component for status indicators
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
        // Subtle tinted background with accent color text
        let bg_color = Hsla {
            a: 0.15,
            ..self.color
        };

        div()
            .bg(bg_color)
            .px(self.design.spacing.sm)
            .py(px(3.0))
            .rounded(self.design.radius.xs)
            .child(
                div()
                    .text_color(self.color)
                    .text_size(self.design.typography.size_xs)
                    .font_weight(FontWeight::BOLD)
                    .child(self.label.clone()),
            )
    }
}

/// Stat display component
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
        div()
            .bg(self.design.colors.bg_secondary)
            .border_1()
            .border_color(self.design.colors.border_secondary)
            .rounded(self.design.radius.md)
            .p(self.design.spacing.md)
            .flex()
            .flex_col()
            .gap(self.design.spacing.xs)
            .flex_1()
            .child(
                div()
                    .text_color(self.design.colors.text_tertiary)
                    .text_size(self.design.typography.size_xs)
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.label.clone()),
            )
            .child(
                div()
                    .text_color(self.design.colors.text_primary)
                    .text_size(self.design.typography.size_xl)
                    .font_weight(FontWeight::BOLD)
                    .child(self.value.clone()),
            )
    }
}

/// Checkbox component
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
        let checked = self.checked;
        let accent_gradient = Gradients::blue_purple(&self.design.colors);
        let bg = self.design.colors.bg_primary;
        let text_primary = self.design.colors.text_primary;
        let text_xs = self.design.typography.size_xs;

        div()
            .id(id)
            .flex()
            .items_center()
            .gap(self.design.spacing.sm)
            .cursor_pointer()
            .on_click(on_toggle)
            .child(
                div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .border_1()
                    .border_color(if checked {
                        self.design.colors.accent_blue
                    } else {
                        self.design.colors.border_primary
                    })
                    .rounded(self.design.radius.xs)
                    .when(checked, |d| d.bg(accent_gradient))
                    .when(!checked, |d| d.bg(bg))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |d| {
                        d.child(div().text_color(text_primary).text_size(text_xs).child("✓"))
                    }),
            )
            .child(
                div()
                    .text_color(self.design.colors.text_secondary)
                    .text_size(self.design.typography.size_sm)
                    .child(self.label.clone()),
            )
    }
}

/// Input field component
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
        div()
            .bg(self.design.colors.bg_primary)
            .border_1()
            .border_color(self.design.colors.border_secondary)
            .rounded(self.design.radius.sm)
            .px(self.design.spacing.md)
            .py(self.design.spacing.sm)
            .child(
                div()
                    .text_color(if self.value.is_empty() {
                        self.design.colors.text_tertiary
                    } else {
                        self.design.colors.text_primary
                    })
                    .text_size(self.design.typography.size_md)
                    .child(if self.value.is_empty() {
                        self.placeholder.clone()
                    } else {
                        self.value.clone()
                    }),
            )
    }
}

/// Separator - gradient accent line
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

/// Progress bar - indeterminate
pub struct ProgressBar {
    design: DesignSystem,
}

impl ProgressBar {
    pub fn new(design: DesignSystem) -> Self {
        Self { design }
    }

    pub fn render_indeterminate(&self) -> Div {
        div()
            .w_full()
            .h(px(3.0))
            .rounded(self.design.radius.xs)
            .bg(self.design.colors.bg_tertiary)
            .child(
                div()
                    .h_full()
                    .w_full()
                    .rounded(self.design.radius.xs)
                    .bg(Gradients::blue_purple(&self.design.colors)),
            )
    }
}
