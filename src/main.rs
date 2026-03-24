mod agent;
mod app;
mod app_state;
mod channels;
mod config;
mod controller;
mod global_hotkey;
mod history;
mod keyring_store;
mod logger;
mod messages;
mod model_downloader;
mod recorder;
mod theme;
mod transcriber;
mod ui;

fn main() {
    let app = match app::App::build() {
        Ok(app) => app,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    if let Err(err) = app.run() {
        eprintln!("{err}");
    }
}
