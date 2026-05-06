use std::io::{self, Write};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub struct Spinner {
    handle: JoinHandle<()>,
    stop_tx: Option<oneshot::Sender<()>>,
}

impl Spinner {
    pub fn start(message: &str) -> Self {
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let message = message.to_string();
        let handle = tokio::spawn(async move {
            let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut frame_index = 0usize;
            let mut interval = tokio::time::interval(Duration::from_millis(100));

            loop {
                tokio::select! {
                    _ = &mut stop_rx => {
                        clear_spinner_line();
                        break;
                    }
                    _ = interval.tick() => {
                        render_spinner_frame(frames[frame_index], &message);
                        frame_index = (frame_index + 1) % frames.len();
                    }
                }
            }
        });

        Self {
            handle,
            stop_tx: Some(stop_tx),
        }
    }

    pub fn stop(mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }

        tokio::spawn(async move {
            let _ = self.handle.await;
        });
    }
}

fn render_spinner_frame(frame: char, message: &str) {
    let mut stdout = io::stdout();
    let _ = write!(stdout, "\r  {frame} {message}");
    let _ = stdout.flush();
}

fn clear_spinner_line() {
    let mut stdout = io::stdout();
    let _ = write!(stdout, "\r\x1b[2K\r");
    let _ = stdout.flush();
}