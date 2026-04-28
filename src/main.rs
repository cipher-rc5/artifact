// file: src/main.rs
// description: GPUI application entry point with bento box UI
// reference: https://github.com/zed-industries/zed

mod app;
mod view;

use app::ArtifactApp;
use artifact::{AppConfig, LoggingConfig};
use gpui::*;
use tracing::info;
use view::ArtifactView;

fn main() -> anyhow::Result<()> {
    let config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("Failed to load configuration, using defaults: {}", e);
        AppConfig::default()
    });

    let logging_config = LoggingConfig {
        log_dir: config.get_log_dir(),
        log_level: config.get_log_level(),
        log_to_file: config.logging.log_to_file,
        log_to_stdout: config.logging.log_to_stdout,
        json_format: config.logging.json_format,
    };

    let _guard = match artifact::logging::init_logging(logging_config) {
        Ok(guard) => {
            info!("ARTIFACT starting up with GPUI");
            guard
        }
        Err(e) => {
            eprintln!("Failed to initialize logging: {}", e);
            std::process::exit(1);
        }
    };

    info!("Configuration loaded successfully");
    info!("Initializing GPUI");

    let window_width = config.ui.window_width;
    let window_height = config.ui.window_height;

    Application::new().run(move |cx: &mut App| {
        let app_model = ArtifactApp::new(config.clone(), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point {
                        x: px(100.0),
                        y: px(100.0),
                    },
                    size: size(px(window_width), px(window_height)),
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("ARTIFACT".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| cx.new(|cx| ArtifactView::new(app_model.clone(), window, cx)),
        )
        .expect("Failed to open window");
    });

    info!("Application shutdown normally");
    Ok(())
}
