// file: src/view.rs
// description: GPUI view for ARTIFACT — sleek dark UI with gradient accents

use gpui::prelude::FluentBuilder;
use gpui::*;
use std::path::PathBuf;
use std::time::Duration;

use crate::app::{ScanState, ArtifactApp};
use artifact::components::*;
use artifact::directory_item::DirectoryType;
use artifact::theme::{DesignSystem, Gradients};
use artifact::utils;

pub struct ArtifactView {
    app: Entity<ArtifactApp>,
    design: DesignSystem,
}

impl ArtifactView {
    pub fn new(app: Entity<ArtifactApp>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_view, _entity, cx| cx.notify()).detach();

        // Poll scan progress in background
        let app_clone = app.clone();
        cx.spawn(async move |_this, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let ok = cx.update(|cx| {
                    app_clone.update(cx, |app, cx| app.check_scan_progress(cx));
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            app,
            design: DesignSystem::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for ArtifactView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let d = self.design;

        // ---- snapshot state ------------------------------------------------
        let scan_state = app.scan_state();
        let scan_path = app.scan_path().to_string();
        let is_scanning = scan_state == ScanState::Scanning;
        let total_size = app.total_size();
        let selected_size = app.selected_size();
        let deleted_count = app.deleted_count();
        let error_msg = app.error_message().map(|s| s.to_string());
        let scan_node_modules = app.scan_node_modules();
        let scan_rust_target = app.scan_rust_target();
        let show_orphaned_only = app.show_orphaned_only();
        let progress_data = app.scan_progress_data().cloned();
        let file_browser_open = app.is_file_browser_open();
        let browse_path_display = app.browse_path().display().to_string();
        let browse_entries: Vec<_> = app
            .browse_entries()
            .iter()
            .map(|e| (e.name.clone(), e.path.clone()))
            .collect();

        let dir_entries: Vec<_> = app
            .visible_entries()
            .iter()
            .map(|(i, item)| {
                (
                    *i,
                    item.path.display().to_string(),
                    item.dir_type.clone(),
                    item.project_name.clone().unwrap_or_default(),
                    item.size_bytes,
                    item.selected,
                    item.is_orphaned,
                )
            })
            .collect();
        let has_entries = !dir_entries.is_empty();
        let _ = app;

        // ---- clone handles for closures ------------------------------------
        let app_scan = self.app.clone();
        let app_sel_all = self.app.clone();
        let app_sel_none = self.app.clone();
        let app_delete = self.app.clone();
        let app_nm = self.app.clone();
        let app_rt = self.app.clone();
        let app_orph = self.app.clone();
        let app_browse_open = self.app.clone();

        // ---- main layout ---------------------------------------------------
        div()
            .size_full()
            .bg(d.colors.bg_primary)
            .text_color(d.colors.text_primary)
            .p(d.spacing.lg)
            .flex()
            .flex_col()
            .gap(d.spacing.md)
            // Header
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(d.spacing.xs)
                    .child(
                        div()
                            .text_size(d.typography.size_title)
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .child("ARTIFACT"),
                    )
                    .child(
                        div()
                            .text_color(d.colors.text_tertiary)
                            .text_size(d.typography.size_sm)
                            .child("Find and remove build artifacts to reclaim disk space"),
                    ),
            )
            // Gradient accent separator
            .child(
                div()
                    .h(px(2.0))
                    .w_full()
                    .bg(Gradients::header())
                    .rounded(d.radius.xs),
            )
            // Scan controls card
            .child(BentoCard::new(d).render(|card| {
                card.flex()
                    .flex_col()
                    .gap(d.spacing.sm)
                    .child(
                        div()
                            .text_size(d.typography.size_md)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(d.colors.text_secondary)
                            .child("SCAN CONTROLS"),
                    )
                    // Path row: display + browse button
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(d.spacing.sm)
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new("Scan path...", &scan_path, d).render()),
                            )
                            .child(
                                Button::new("Browse", d)
                                    .variant(ButtonVariant::Secondary)
                                    .disabled(is_scanning)
                                    .render("btn-browse", move |_, _, cx| {
                                        app_browse_open.update(cx, |a, cx| {
                                            a.open_file_browser(cx);
                                        });
                                    }),
                            ),
                    )
                    // Checkboxes
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(d.spacing.md)
                            .child(Checkbox::new("node_modules", scan_node_modules, d).render(
                                "cb-nm",
                                move |_, _, cx| {
                                    app_nm.update(cx, |a, cx| a.toggle_node_modules(cx));
                                },
                            ))
                            .child(Checkbox::new("rust target", scan_rust_target, d).render(
                                "cb-rt",
                                move |_, _, cx| {
                                    app_rt.update(cx, |a, cx| a.toggle_rust_target(cx));
                                },
                            ))
                            .child(
                                Checkbox::new("orphaned only", show_orphaned_only, d).render(
                                    "cb-orph",
                                    move |_, _, cx| {
                                        app_orph.update(cx, |a, cx| a.toggle_orphaned_only(cx));
                                    },
                                ),
                            ),
                    )
                    // Buttons
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(d.spacing.sm)
                            .child(Button::new("Scan", d).disabled(is_scanning).render(
                                "btn-scan",
                                move |_, _, cx| {
                                    app_scan.update(cx, |a, cx| a.start_scan(cx));
                                },
                            ))
                            .child(
                                Button::new("Select All", d)
                                    .variant(ButtonVariant::Secondary)
                                    .disabled(!has_entries)
                                    .render("btn-sel-all", move |_, _, cx| {
                                        app_sel_all.update(cx, |a, cx| a.select_all(cx));
                                    }),
                            )
                            .child(
                                Button::new("Select None", d)
                                    .variant(ButtonVariant::Ghost)
                                    .disabled(!has_entries)
                                    .render("btn-sel-none", move |_, _, cx| {
                                        app_sel_none.update(cx, |a, cx| a.select_none(cx));
                                    }),
                            ),
                    )
            }))
            // File browser (conditional)
            .when(file_browser_open, |root| {
                root.child(self.render_file_browser(d, &browse_path_display, &browse_entries))
            })
            // Scanning progress (conditional)
            .when(is_scanning, |root| {
                root.child(Self::render_progress_panel(d, progress_data.as_ref()))
            })
            // Stats row (when not scanning)
            .when(!is_scanning, |root| {
                root.child(
                    div()
                        .flex()
                        .gap(d.spacing.sm)
                        .child(
                            StatBox::new("Total Size", utils::format_size(total_size), d).render(),
                        )
                        .child(
                            StatBox::new("Selected", utils::format_size(selected_size), d).render(),
                        )
                        .child(StatBox::new("Deleted", deleted_count.to_string(), d).render()),
                )
            })
            // Directory list
            .when(!is_scanning, |root| {
                root.child(Self::render_directory_list(
                    d,
                    scan_state,
                    &dir_entries,
                    &self.app,
                ))
            })
            // Delete button
            .when(!is_scanning && has_entries, |root| {
                root.child(
                    Button::new("Delete Selected", d)
                        .variant(ButtonVariant::Danger)
                        .disabled(selected_size == 0)
                        .render("btn-delete", move |_, _, cx| {
                            app_delete.update(cx, |a, cx| a.delete_selected(cx));
                        }),
                )
            })
            // Error
            .when(error_msg.is_some(), |root| {
                let msg = error_msg.unwrap_or_default();
                root.child(
                    div()
                        .bg(hsla(355.0, 0.80, 0.50, 0.15))
                        .border_1()
                        .border_color(d.colors.status_error)
                        .text_color(d.colors.status_error)
                        .rounded(d.radius.sm)
                        .px(d.spacing.md)
                        .py(d.spacing.sm)
                        .text_size(d.typography.size_sm)
                        .child(msg),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Sub-renderers (static helpers to keep render() compact)
// ---------------------------------------------------------------------------

impl ArtifactView {
    /// Progress panel shown while a scan is running.
    fn render_progress_panel(d: DesignSystem, progress: Option<&crate::app::ScanProgress>) -> Div {
        let (dirs_scanned, items_found, current_path, size_found, elapsed) = match progress {
            Some(p) => (
                p.directories_scanned,
                p.items_found,
                p.current_path.clone(),
                p.total_size_found,
                p.elapsed_secs,
            ),
            None => (0, 0, String::new(), 0, 0.0),
        };

        let rate = if elapsed > 0.5 {
            format!("{:.0} dirs/s", dirs_scanned as f64 / elapsed)
        } else {
            "--".to_string()
        };

        // Truncate long paths for display
        let display_path = if current_path.len() > 80 {
            let start = current_path.len() - 77;
            format!("...{}", &current_path[start..])
        } else {
            current_path.clone()
        };

        div()
            .bg(d.colors.bg_secondary)
            .border_1()
            .border_color(d.colors.border_primary)
            .rounded(d.radius.md)
            .p(d.spacing.md)
            .flex()
            .flex_col()
            .gap(d.spacing.sm)
            // Title
            .child(
                div()
                    .text_size(d.typography.size_md)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(d.colors.text_secondary)
                    .child("SCANNING"),
            )
            // Progress bar
            .child(ProgressBar::new(d).render_indeterminate())
            // Stats grid
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(d.spacing.xs)
                    .child(Self::stat_row(
                        d,
                        "Elapsed",
                        &utils::format_elapsed(elapsed),
                    ))
                    .child(Self::stat_row(d, "Scan rate", &rate))
                    .child(Self::stat_row(
                        d,
                        "Directories scanned",
                        &format_number(dirs_scanned),
                    ))
                    .child(Self::stat_row(
                        d,
                        "Build artifacts found",
                        &items_found.to_string(),
                    ))
                    .child(Self::stat_row(
                        d,
                        "Reclaimable space",
                        &utils::format_size(size_found),
                    )),
            )
            // Current path
            .when(!display_path.is_empty(), |panel| {
                panel.child(
                    div()
                        .text_color(d.colors.text_tertiary)
                        .text_size(d.typography.size_xs)
                        .overflow_x_hidden()
                        .child(display_path),
                )
            })
    }

    /// A label : value row for the progress stats.
    fn stat_row(d: DesignSystem, label: &str, value: &str) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_sm)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_color(d.colors.text_primary)
                    .text_size(d.typography.size_sm)
                    .font_weight(FontWeight::BOLD)
                    .child(value.to_string()),
            )
    }

    /// File browser panel.
    fn render_file_browser(
        &self,
        d: DesignSystem,
        browse_path: &str,
        entries: &[(String, PathBuf)],
    ) -> Div {
        let app_cancel = self.app.clone();
        let app_select = self.app.clone();

        let mut list = div()
            .id("file-browser-list")
            .flex()
            .flex_col()
            .max_h(px(260.0))
            .overflow_y_scroll()
            .bg(d.colors.bg_primary)
            .border_1()
            .border_color(d.colors.border_secondary)
            .rounded(d.radius.sm);

        if entries.is_empty() {
            list = list.child(
                div()
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_sm)
                    .p(d.spacing.md)
                    .child("No subdirectories"),
            );
        } else {
            for (name, path) in entries {
                let app_nav = self.app.clone();
                let nav_path = path.clone();
                let is_parent = name == "..";
                let label = if is_parent {
                    "..  (parent directory)".to_string()
                } else {
                    format!("{}/", name)
                };

                list = list.child(
                    div()
                        .id(ElementId::Name(format!("browse-{}", path.display()).into()))
                        .px(d.spacing.md)
                        .py(d.spacing.xs)
                        .cursor_pointer()
                        .hover(|s| s.bg(d.colors.bg_tertiary))
                        .on_click(move |_, _, cx| {
                            app_nav.update(cx, |a, cx| {
                                a.browse_navigate(nav_path.clone(), cx);
                            });
                        })
                        .child(
                            div()
                                .text_size(d.typography.size_sm)
                                .text_color(if is_parent {
                                    d.colors.text_tertiary
                                } else {
                                    d.colors.text_primary
                                })
                                .child(label),
                        ),
                );
            }
        }

        // Outer card with gradient border accent
        div()
            .bg(Gradients::surface(&d.colors))
            .border_1()
            .border_color(d.colors.accent_blue)
            .rounded(d.radius.md)
            .p(d.spacing.md)
            .flex()
            .flex_col()
            .gap(d.spacing.sm)
            // Header
            .child(
                div()
                    .text_size(d.typography.size_md)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(d.colors.text_secondary)
                    .child("SELECT SCAN DIRECTORY"),
            )
            // Current path
            .child(
                div()
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_sm)
                    .child(browse_path.to_string()),
            )
            // Directory listing
            .child(list)
            // Action buttons
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("Cancel", d)
                            .variant(ButtonVariant::Ghost)
                            .render("btn-browse-cancel", move |_, _, cx| {
                                app_cancel.update(cx, |a, cx| a.close_file_browser(cx));
                            }),
                    )
                    .child(Button::new("Select This Directory", d).render(
                        "btn-browse-select",
                        move |_, _, cx| {
                            app_select.update(cx, |a, cx| a.browse_select(cx));
                        },
                    )),
            )
    }

    /// Scrollable directory results list.
    fn render_directory_list(
        d: DesignSystem,
        scan_state: ScanState,
        entries: &[(usize, String, DirectoryType, String, u64, bool, bool)],
        app: &Entity<ArtifactApp>,
    ) -> Stateful<Div> {
        let mut list = div()
            .id("dir-list")
            .bg(d.colors.bg_secondary)
            .border_1()
            .border_color(d.colors.border_primary)
            .rounded(d.radius.md)
            .p(d.spacing.sm)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .min_h(px(100.0))
            .overflow_y_scroll();

        if entries.is_empty() {
            list = list.child(
                div()
                    .text_color(d.colors.text_tertiary)
                    .text_size(d.typography.size_sm)
                    .p(d.spacing.md)
                    .child(match scan_state {
                        ScanState::Idle => "Click Scan to find build artifacts",
                        ScanState::Scanning => "Scanning...",
                        ScanState::Complete => "No directories found",
                    }),
            );
        } else {
            for (idx, path, dir_type, project_name, size_bytes, selected, is_orphaned) in entries {
                let app_toggle = app.clone();
                let idx = *idx;
                let badge_color = match dir_type {
                    DirectoryType::NodeModules => d.colors.accent_green,
                    DirectoryType::RustTarget => d.colors.accent_orange,
                };
                let badge_label = dir_type.to_string();
                let size_str = utils::format_size(*size_bytes);
                let selected = *selected;
                let is_orphaned = *is_orphaned;

                // Gradient checkbox for selected items
                let checkbox_bg = if selected {
                    Gradients::blue_purple(&d.colors)
                } else {
                    gpui::linear_gradient(
                        0.0,
                        gpui::linear_color_stop(d.colors.bg_primary, 0.0),
                        gpui::linear_color_stop(d.colors.bg_primary, 1.0),
                    )
                };

                list = list.child(
                    div()
                        .id(ElementId::Name(format!("dir-{idx}").into()))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(d.spacing.sm)
                        .py(d.spacing.xs)
                        .rounded(d.radius.sm)
                        .cursor_pointer()
                        .hover(|s| s.bg(d.colors.bg_tertiary))
                        .when(selected, |el| el.bg(hsla(220.0, 0.85, 0.55, 0.06)))
                        .on_click(move |_, _, cx| {
                            app_toggle.update(cx, |a, cx| a.toggle_selection(idx, cx));
                        })
                        // Left side: checkbox + info
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(d.spacing.sm)
                                .flex_1()
                                .child(
                                    div()
                                        .w(px(14.0))
                                        .h(px(14.0))
                                        .border_1()
                                        .border_color(if selected {
                                            d.colors.accent_blue
                                        } else {
                                            d.colors.border_primary
                                        })
                                        .rounded(d.radius.xs)
                                        .bg(checkbox_bg)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(selected, |c| {
                                            c.child(
                                                div()
                                                    .text_color(d.colors.text_primary)
                                                    .text_size(px(9.0))
                                                    .child("✓"),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(1.0))
                                        .child(
                                            div()
                                                .text_size(d.typography.size_sm)
                                                .child(path.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(d.spacing.sm)
                                                .child(
                                                    div()
                                                        .text_color(d.colors.text_tertiary)
                                                        .text_size(d.typography.size_xs)
                                                        .child(size_str),
                                                )
                                                .when(!project_name.is_empty(), |row| {
                                                    row.child(
                                                        div()
                                                            .text_color(d.colors.text_tertiary)
                                                            .text_size(d.typography.size_xs)
                                                            .child(project_name.clone()),
                                                    )
                                                })
                                                .when(is_orphaned, |row| {
                                                    row.child(
                                                        Badge::new(
                                                            "orphaned",
                                                            d.colors.accent_red,
                                                            d,
                                                        )
                                                        .render(),
                                                    )
                                                }),
                                        ),
                                ),
                        )
                        // Right side: type badge
                        .child(Badge::new(badge_label, badge_color, d).render()),
                );
            }
        }

        list
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_number(n: usize) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
