use std::io::{self, BufRead};

/// Boxed handler invoked for a matching stdin command.
pub type CommandHandler = Box<dyn FnMut() + 'static>;

/// Starts a background loop reading stdin line by line and invoking matching handlers.
/// The listener prints available commands and their descriptions before running.
pub fn start(commands: Vec<(String, String, CommandHandler)>) {
    println!("Commands:");

    let mut handlers = std::collections::HashMap::new();
    for (op, description, handler) in commands {
        println!("  {op:<8} - {description}");
        handlers.insert(op, handler);
    }
    println!("  (Ctrl+D to exit)");

    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());

    loop {
        let mut line = String::new();
        let bytes = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(err) => {
                eprintln!("stdin listener error: {err}");
                break;
            }
        };

        if bytes == 0 {
            // EOF.
            break;
        }

        let command = line.trim();
        if let Some(handler) = handlers.get_mut(command) {
            handler();
        } else if !command.is_empty() {
            eprintln!("unrecognized command: {command}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_alias_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<CommandHandler>();
    }
}
