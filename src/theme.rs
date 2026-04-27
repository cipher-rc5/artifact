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
        // gpui's hsla() takes h in 0–1, NOT 0–360 degrees. Divide degrees by 360.
        let green = hsla(138.0 / 360.0, 1.0, 0.52, 1.0);
        Self {
            bg_primary:       hsla(0.0, 0.0, 0.03, 1.0), // #080808
            bg_secondary:     hsla(0.0, 0.0, 0.07, 1.0), // #121212 — panels, sidebar
            bg_tertiary:      hsla(0.0, 0.0, 0.12, 1.0), // #1F1F1F — hover
            bg_elevated:      hsla(0.0, 0.0, 0.09, 1.0), // #171717
            border_primary:   hsla(0.0, 0.0, 0.18, 1.0), // #2E2E2E — visible hairlines
            border_secondary: hsla(0.0, 0.0, 0.11, 1.0), // #1C1C1C — subtle dividers
            border_focus:     green,
            text_primary:     hsla(0.0, 0.0, 0.84, 1.0), // #D6D6D6
            text_secondary:   hsla(0.0, 0.0, 0.55, 1.0), // #8C8C8C
            text_tertiary:    hsla(0.0, 0.0, 0.40, 1.0), // #666666
            text_inverse:     hsla(0.0, 0.0, 0.05, 1.0),
            accent_green:     green,
            accent_orange:    hsla(28.0  / 360.0, 1.0, 0.55, 1.0),
            accent_red:       hsla(2.0   / 360.0, 0.9, 0.52, 1.0),
            accent_blue:      hsla(195.0 / 360.0, 1.0, 0.52, 1.0),
            accent_yellow:    hsla(50.0  / 360.0, 1.0, 0.55, 1.0),
            accent_purple:    hsla(270.0 / 360.0, 0.8, 0.60, 1.0),
            status_success:   green,
            status_warning:   hsla(28.0  / 360.0, 1.0, 0.55, 1.0),
            status_error:     hsla(2.0   / 360.0, 0.9, 0.52, 1.0),
            status_info:      hsla(195.0 / 360.0, 1.0, 0.52, 1.0),
            interactive_hover:    hsla(0.0, 0.0, 0.11, 1.0),
            interactive_active:   hsla(0.0, 0.0, 0.16, 1.0),
            interactive_disabled: hsla(0.0, 0.0, 0.18, 1.0),
        }
    }
}

pub struct Gradients;

impl Gradients {
    pub fn blue_purple(_theme: &BentoTheme) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(hsla(138.0 / 360.0, 0.5, 0.10, 1.0), 0.0),
            linear_color_stop(hsla(138.0 / 360.0, 0.4, 0.08, 1.0), 1.0),
        )
    }

    pub fn red_orange(_theme: &BentoTheme) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(hsla(2.0 / 360.0, 0.4, 0.12, 1.0), 0.0),
            linear_color_stop(hsla(2.0 / 360.0, 0.3, 0.10, 1.0), 1.0),
        )
    }

    pub fn green_teal(_theme: &BentoTheme) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(hsla(138.0 / 360.0, 0.5, 0.10, 1.0), 0.0),
            linear_color_stop(hsla(138.0 / 360.0, 0.4, 0.08, 1.0), 1.0),
        )
    }

    pub fn surface(theme: &BentoTheme) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(theme.bg_secondary, 0.0),
            linear_color_stop(theme.bg_elevated, 1.0),
        )
    }

    pub fn header() -> Background {
        linear_gradient(
            90.0,
            linear_color_stop(hsla(138.0 / 360.0, 1.0, 0.52, 1.0), 0.0),
            linear_color_stop(hsla(165.0 / 360.0, 0.9, 0.42, 1.0), 1.0),
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
            xs: px(4.0),
            sm: px(8.0),
            md: px(14.0),
            lg: px(20.0),
            xl: px(28.0),
            xxl: px(40.0),
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
            size_xs:    px(10.0),
            size_sm:    px(11.0),
            size_md:    px(12.0),
            size_lg:    px(13.0),
            size_xl:    px(20.0),
            size_xxl:   px(24.0),
            size_title: px(22.0),
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
            xs: px(1.0),
            sm: px(2.0),
            md: px(3.0),
            lg: px(4.0),
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
