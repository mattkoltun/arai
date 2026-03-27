mod app;
mod app_state;
mod channels;
mod config;
mod controller;
mod global_hotkey;
mod history;
mod keyring_store;
mod llm;
mod logger;
mod messages;
mod model_downloader;
mod openai_connector;
mod recorder;
mod theme;
mod transcriber;
mod ui;

fn main() {
    let exit_code = match app::App::build() {
        Ok(app) => match app.run() {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("{err}");
                1
            }
        },
        Err(err) => {
            eprintln!("{err}");
            1
        }
    };

    #[cfg(target_os = "macos")]
    {
        // Work around a ggml Metal teardown assert that can fire during the
        // process-wide C atexit phase even after Arai has shut down cleanly.
        unsafe { libc::_exit(exit_code) };
    }

    #[cfg(not(target_os = "macos"))]
    std::process::exit(exit_code);
}
