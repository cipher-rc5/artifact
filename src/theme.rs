use gpui::{Background, Hsla, Pixels, hsla, linear_color_stop, linear_gradient, px};

#[derive(Debug, Clone, Copy)]
pub struct BentoTheme {
    pub bg_primary: Hsla,
    pub bg_secondary: Hsla,
    pub bg_tertiary: Hsla,
    pub bg_elevated: Hsla,
    pub border_primary: Hsla,
    pub border_secondary: Hsla,
    pub border_focus: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub text_inverse: Hsla,
    pub accent_green: Hsla,
    pub accent_orange: Hsla,
    pub accent_red: Hsla,
    pub accent_blue: Hsla,
    pub accent_yellow: Hsla,
    pub accent_purple: Hsla,
    pub status_success: Hsla,
    pub status_warning: Hsla,
    pub status_error: Hsla,
    pub status_info: Hsla,
    pub interactive_hover: Hsla,
    pub interactive_active: Hsla,
    pub interactive_disabled: Hsla,
}

impl Default for BentoTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl BentoTheme {
    pub fn dark() -> Self {
        // gpui's hsla() takes h in 0–1, NOT 0–360.
        let green = hsla(150.0 / 360.0, 0.62, 0.58, 1.0);
        let orange = hsla(28.0 / 360.0, 0.85, 0.62, 1.0);
        let red = hsla(2.0 / 360.0, 0.78, 0.62, 1.0);
        let blue = hsla(212.0 / 360.0, 0.72, 0.65, 1.0);
        Self {
            // Calmer, slightly cool-tinted dark surfaces; more separation between
            // the canvas (bg_primary) and the floating cards (bg_secondary).
            bg_primary: hsla(220.0 / 360.0, 0.06, 0.055, 1.0), // canvas
            bg_secondary: hsla(220.0 / 360.0, 0.05, 0.10, 1.0), // card surface
            bg_tertiary: hsla(220.0 / 360.0, 0.05, 0.14, 1.0), // hover / nested
            bg_elevated: hsla(220.0 / 360.0, 0.05, 0.12, 1.0), // popovers
            border_primary: hsla(220.0 / 360.0, 0.05, 0.16, 1.0),
            border_secondary: hsla(220.0 / 360.0, 0.04, 0.12, 1.0),
            border_focus: green,
            text_primary: hsla(220.0 / 360.0, 0.05, 0.94, 1.0),
            text_secondary: hsla(220.0 / 360.0, 0.04, 0.66, 1.0),
            text_tertiary: hsla(220.0 / 360.0, 0.04, 0.46, 1.0),
            text_inverse: hsla(0.0, 0.0, 0.05, 1.0),
            accent_green: green,
            accent_orange: orange,
            accent_red: red,
            accent_blue: blue,
            accent_yellow: hsla(48.0 / 360.0, 0.85, 0.65, 1.0),
            accent_purple: hsla(268.0 / 360.0, 0.55, 0.70, 1.0),
            status_success: green,
            status_warning: orange,
            status_error: red,
            status_info: blue,
            interactive_hover: hsla(220.0 / 360.0, 0.05, 0.16, 1.0),
            interactive_active: hsla(220.0 / 360.0, 0.05, 0.20, 1.0),
            interactive_disabled: hsla(220.0 / 360.0, 0.04, 0.18, 1.0),
        }
    }
}

pub struct Gradients;

impl Gradients {
    pub fn green_card(_theme: &BentoTheme) -> Background {
        linear_gradient(
            155.0,
            linear_color_stop(hsla(150.0 / 360.0, 0.45, 0.16, 1.0), 0.0),
            linear_color_stop(hsla(168.0 / 360.0, 0.40, 0.10, 1.0), 1.0),
        )
    }

    pub fn warm_card(_theme: &BentoTheme) -> Background {
        linear_gradient(
            155.0,
            linear_color_stop(hsla(18.0 / 360.0, 0.45, 0.16, 1.0), 0.0),
            linear_color_stop(hsla(2.0 / 360.0, 0.40, 0.11, 1.0), 1.0),
        )
    }

    pub fn cool_card(_theme: &BentoTheme) -> Background {
        linear_gradient(
            155.0,
            linear_color_stop(hsla(218.0 / 360.0, 0.40, 0.16, 1.0), 0.0),
            linear_color_stop(hsla(232.0 / 360.0, 0.38, 0.10, 1.0), 1.0),
        )
    }

    pub fn surface(theme: &BentoTheme) -> Background {
        linear_gradient(
            165.0,
            linear_color_stop(theme.bg_secondary, 0.0),
            linear_color_stop(theme.bg_elevated, 1.0),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    pub xs: Pixels,
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
    pub xl: Pixels,
    pub xxl: Pixels,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: px(6.0),
            sm: px(10.0),
            md: px(16.0),
            lg: px(24.0),
            xl: px(36.0),
            xxl: px(56.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Typography {
    pub size_xs: Pixels,
    pub size_sm: Pixels,
    pub size_md: Pixels,
    pub size_lg: Pixels,
    pub size_xl: Pixels,
    pub size_xxl: Pixels,
    pub size_title: Pixels,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            size_xs: px(11.0),
            size_sm: px(12.0),
            size_md: px(13.0),
            size_lg: px(15.0),
            size_xl: px(22.0),
            size_xxl: px(30.0),
            size_title: px(40.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BorderRadius {
    pub xs: Pixels,
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self {
            xs: px(6.0),
            sm: px(10.0),
            md: px(14.0),
            lg: px(20.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DesignSystem {
    pub colors: BentoTheme,
    pub spacing: Spacing,
    pub typography: Typography,
    pub radius: BorderRadius,
}

impl DesignSystem {
    pub fn new() -> Self {
        Self::default()
    }
}
