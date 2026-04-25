// file: src/theme.rs
// description: Sleek modern dark theme with gradients and sharp edges

use gpui::{Background, Hsla, Pixels, hsla, linear_color_stop, linear_gradient, px};

/// Modern dark color palette - deep blacks with vibrant accent gradients
#[derive(Debug, Clone, Copy)]
pub struct BentoTheme {
    // Background colors - deep layered blacks
    pub bg_primary: Hsla,   // Main background - near black
    pub bg_secondary: Hsla, // Card backgrounds - slightly lifted
    pub bg_tertiary: Hsla,  // Hover states
    pub bg_elevated: Hsla,  // Modals, popovers

    // Border colors
    pub border_primary: Hsla,   // Main borders - subtle
    pub border_secondary: Hsla, // Subtle dividers
    pub border_focus: Hsla,     // Focus rings

    // Text colors
    pub text_primary: Hsla,   // Main text - crisp white
    pub text_secondary: Hsla, // Secondary text - muted
    pub text_tertiary: Hsla,  // Disabled/placeholder
    pub text_inverse: Hsla,   // Text on accent colors

    // Accent colors - vibrant, modern
    pub accent_green: Hsla,  // node_modules indicator
    pub accent_orange: Hsla, // rust target indicator
    pub accent_red: Hsla,    // Danger/delete
    pub accent_blue: Hsla,   // Info/links
    pub accent_yellow: Hsla, // Warning
    pub accent_purple: Hsla, // Highlight/feature accent

    // Status colors
    pub status_success: Hsla,
    pub status_warning: Hsla,
    pub status_error: Hsla,
    pub status_info: Hsla,

    // Interactive states
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
    /// Sleek dark theme - deep blacks with vibrant accents
    pub fn dark() -> Self {
        Self {
            // Backgrounds - true deep blacks for contrast
            bg_primary: hsla(240.0, 0.06, 0.05, 1.0), // #0B0B0F - near black with cool tint
            bg_secondary: hsla(240.0, 0.05, 0.08, 1.0), // #121216 - card surface
            bg_tertiary: hsla(240.0, 0.04, 0.12, 1.0), // #1C1C21 - hover/elevated
            bg_elevated: hsla(240.0, 0.04, 0.15, 1.0), // #242429 - modals

            // Borders - razor thin, low contrast
            border_primary: hsla(240.0, 0.04, 0.18, 1.0), // #2B2B30
            border_secondary: hsla(240.0, 0.03, 0.12, 1.0), // #1D1D20
            border_focus: hsla(220.0, 0.85, 0.55, 1.0),   // Electric blue focus

            // Text - high contrast crisp whites
            text_primary: hsla(0.0, 0.0, 0.96, 1.0), // #F5F5F5
            text_secondary: hsla(0.0, 0.0, 0.55, 1.0), // #8C8C8C
            text_tertiary: hsla(0.0, 0.0, 0.35, 1.0), // #595959
            text_inverse: hsla(0.0, 0.0, 0.03, 1.0), // Near black

            // Accents - vibrant and electric
            accent_green: hsla(155.0, 0.75, 0.45, 1.0), // #1CC775 - electric emerald
            accent_orange: hsla(28.0, 0.90, 0.55, 1.0), // #F07830 - hot orange
            accent_red: hsla(355.0, 0.80, 0.50, 1.0),   // #E6243D - vivid red
            accent_blue: hsla(220.0, 0.85, 0.55, 1.0),  // #3375F0 - electric blue
            accent_yellow: hsla(48.0, 0.90, 0.55, 1.0), // #F0B810 - amber
            accent_purple: hsla(265.0, 0.70, 0.55, 1.0), // #8033E0 - vivid purple

            // Status
            status_success: hsla(155.0, 0.75, 0.45, 1.0),
            status_warning: hsla(48.0, 0.90, 0.55, 1.0),
            status_error: hsla(355.0, 0.80, 0.50, 1.0),
            status_info: hsla(220.0, 0.85, 0.55, 1.0),

            // Interactive
            interactive_hover: hsla(240.0, 0.06, 0.14, 1.0), // Subtle lift
            interactive_active: hsla(240.0, 0.06, 0.18, 1.0), // Pressed
            interactive_disabled: hsla(0.0, 0.0, 0.20, 1.0),
        }
    }
}

/// Gradient presets for the UI
pub struct Gradients;

impl Gradients {
    /// Blue to purple gradient - primary action buttons
    pub fn blue_purple(theme: &BentoTheme) -> Background {
        linear_gradient(
            135.0,
            linear_color_stop(theme.accent_blue, 0.0),
            linear_color_stop(theme.accent_purple, 1.0),
        )
    }

    /// Red to orange gradient - danger/delete buttons
    pub fn red_orange(theme: &BentoTheme) -> Background {
        linear_gradient(
            135.0,
            linear_color_stop(theme.accent_red, 0.0),
            linear_color_stop(hsla(15.0, 0.85, 0.45, 1.0), 1.0),
        )
    }

    /// Green to teal gradient - success states
    pub fn green_teal(theme: &BentoTheme) -> Background {
        linear_gradient(
            135.0,
            linear_color_stop(theme.accent_green, 0.0),
            linear_color_stop(hsla(180.0, 0.65, 0.40, 1.0), 1.0),
        )
    }

    /// Subtle surface gradient - cards and elevated surfaces
    pub fn surface(theme: &BentoTheme) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(theme.bg_secondary, 0.0),
            linear_color_stop(theme.bg_tertiary, 1.0),
        )
    }

    /// Header gradient - title area accent
    pub fn header() -> Background {
        linear_gradient(
            90.0,
            linear_color_stop(hsla(220.0, 0.85, 0.55, 1.0), 0.0),
            linear_color_stop(hsla(265.0, 0.70, 0.55, 1.0), 1.0),
        )
    }
}

/// Spacing system - consistent rhythm
#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    pub xs: Pixels,  // 4px
    pub sm: Pixels,  // 8px
    pub md: Pixels,  // 16px
    pub lg: Pixels,  // 24px
    pub xl: Pixels,  // 32px
    pub xxl: Pixels, // 48px
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: px(4.0),
            sm: px(8.0),
            md: px(16.0),
            lg: px(24.0),
            xl: px(32.0),
            xxl: px(48.0),
        }
    }
}

/// Typography system - clean, readable
#[derive(Debug, Clone, Copy)]
pub struct Typography {
    pub size_xs: Pixels,    // 11px
    pub size_sm: Pixels,    // 13px
    pub size_md: Pixels,    // 14px
    pub size_lg: Pixels,    // 16px
    pub size_xl: Pixels,    // 20px
    pub size_xxl: Pixels,   // 24px
    pub size_title: Pixels, // 32px
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            size_xs: px(11.0),
            size_sm: px(13.0),
            size_md: px(14.0),
            size_lg: px(16.0),
            size_xl: px(20.0),
            size_xxl: px(24.0),
            size_title: px(32.0),
        }
    }
}

/// Border radius system - sharp, modern edges
#[derive(Debug, Clone, Copy)]
pub struct BorderRadius {
    pub xs: Pixels, // 2px - crisp
    pub sm: Pixels, // 3px - subtle
    pub md: Pixels, // 5px - cards
    pub lg: Pixels, // 6px - containers
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self {
            xs: px(2.0),
            sm: px(3.0),
            md: px(5.0),
            lg: px(6.0),
        }
    }
}

/// Complete design system
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
