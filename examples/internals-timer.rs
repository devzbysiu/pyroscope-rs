extern crate pyroscope;

use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};

use pyroscope::timer::Timer;

fn main() -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
    // Initialize the Timer
    let mut timer = Timer::initialize()?;

    // Create a streaming channel
    let (tx, rx): (Sender<u64>, Receiver<u64>) = channel();

    let (tx2, rx2): (Sender<u64>, Receiver<u64>) = channel();

    // Attach tx to Timer
    timer.attach_listener(tx)?;
    timer.attach_listener(tx2)?;

    // Show current time
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    println!("Current Time: {}", now);

    // Listen to the Timer events
    std::thread::spawn(move || {
        #[allow(irrefutable_let_patterns)]
        while let result = rx.recv() {
            match result {
                Ok(time) => println!("Thread 1 Notification: {}", time),
                Err(_err) => {
                    println!("Error Thread 1");
                    break;
                }
            }
        }
    });

    std::thread::spawn(move || {
        #[allow(irrefutable_let_patterns)]
        while let result = rx2.recv() {
            match result {
                Ok(time) => println!("Thread 2 Notification: {}", time),
                Err(_err) => {
                    println!("Error Thread 2");
                    break;
                }
            }
        }
    })
    .join()?;
}
