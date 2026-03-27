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

    std::process::exit(exit_code);
}
