use std::io::{self, Write};

/// Text console shared by every task.
///
/// Output is always recorded so it can be inspected afterwards, and is
/// optionally echoed to the host's stdout as it is written.
pub struct Console {
    buffer: String,
    echo: bool,
}

impl Console {
    pub fn new(echo: bool) -> Self {
        Self {
            buffer: String::new(),
            echo,
        }
    }

    pub fn write_str(&mut self, text: &str) {
        if self.echo {
            let mut stdout = io::stdout().lock();
            let _ = stdout.write_all(text.as_bytes());
            let _ = stdout.flush();
        }
        self.buffer.push_str(text);
    }

    pub fn write_char(&mut self, ch: char) {
        self.write_str(ch.encode_utf8(&mut [0; 4]));
    }

    /// Everything written so far.
    pub fn output(&self) -> &str {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_everything_written() {
        let mut console = Console::new(false);
        console.write_str("tick ");
        console.write_char('7');
        console.write_char('\n');
        assert_eq!(console.output(), "tick 7\n");
    }
}
