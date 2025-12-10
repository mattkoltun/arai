use std::io::{self, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

mod recorder;
mod transcriber;

fn main() {
    println!("Starting microphone recorder with live transcription...");

    let recorder = recorder::Recorder::new();
    let transcriber =
        transcriber::Transcriber::from_default_model().expect("failed to load Whisper model");

    let (audio_tx, audio_rx) = mpsc::channel::<recorder::AudioChunk>();
    let transcript = Arc::new(Mutex::new(String::new()));

    // Spawn transcriber to consume audio and append text into transcript buffer.
    {
        let transcript = Arc::clone(&transcript);
        thread::spawn(move || {
            if let Err(err) = transcriber.transcribe_streaming(audio_rx, transcript) {
                eprintln!("Transcriber error: {err:?}");
            }
        });
    }

    // Spawn printer to stream new transcript text to stdout.
    {
        let transcript = Arc::clone(&transcript);
        thread::spawn(move || {
            let mut last_len = 0;
            loop {
                thread::sleep(Duration::from_millis(500));
                if let Ok(out) = transcript.lock() {
                    if out.len() > last_len {
                        print!("{}", &out[last_len..]);
                        let _ = io::stdout().flush();
                        last_len = out.len();
                    }
                }
            }
        });
    }

    match recorder.start(audio_tx) {
        Ok(()) => println!("Recording started; press Ctrl+C to stop."),
        Err(err) => {
            eprintln!("Failed to start recording: {err:?}");
            return;
        }
    }

    // Keep the main thread alive while recorder and transcriber run.
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
