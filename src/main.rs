mod controller;
mod recorder;
mod transcriber;
mod ui;

fn main() {
    let controller = controller::Controller::new();
    if let Err(err) = ui::run_chat_ui(controller) {
        eprintln!("Failed to launch UI: {err}");
    }
}
