// file: src/main.rs
// description: GPUI application entry point with bento box UI
// reference: https://github.com/zed-industries/zed

#![allow(unexpected_cfgs)]

mod app;
mod view;

use app::ArtifactApp;
use artifact::{AppConfig, LoggingConfig};
use gpui::*;
use tracing::info;
use view::ArtifactView;

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use cocoa::appkit::NSApp;
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSData;
    use objc::{class, msg_send, sel, sel_impl};

    const APP_ICON: &[u8] = include_bytes!("../assets/app-icon.png");

    unsafe {
        let data =
            NSData::dataWithBytes_length_(nil, APP_ICON.as_ptr().cast(), APP_ICON.len() as u64);
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithData:data];

        if image != nil {
            let app = NSApp();
            let _: () = msg_send![app, setApplicationIconImage:image];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_dock_icon() {}

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
        set_dock_icon();

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

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
                app_id: Some("com.cipher.artifact".to_string()),
                window_min_size: Some(size(px(880.0), px(640.0))),
                ..Default::default()
            },
            move |window, cx| cx.new(|cx| ArtifactView::new(app_model.clone(), window, cx)),
        )
        .expect("Failed to open window");
    });

    info!("Application shutdown normally");
    Ok(())
}
