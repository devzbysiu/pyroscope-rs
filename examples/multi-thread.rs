extern crate pyroscope;

use pyroscope::{PyroscopeAgent, Result};
use pyroscope_pprofrs::{Pprof, PprofConfig};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    thread,
};

fn hash_rounds1(n: u64) -> u64 {
    let hash_str = "Some string to hash";
    let mut default_hasher = DefaultHasher::new();

    for _ in 0..n {
        for _ in 0..1000 {
            default_hasher.write(hash_str.as_bytes());
        }
        hash_str.hash(&mut default_hasher);
    }

    n
}

fn hash_rounds2(n: u64) -> u64 {
    let hash_str = "Some string to hash";
    let mut default_hasher = DefaultHasher::new();

    for _ in 0..n {
        for _ in 0..1000 {
            default_hasher.write(hash_str.as_bytes());
        }
        hash_str.hash(&mut default_hasher);
    }

    n
}

fn main() -> Result<()> {
    let mut agent = PyroscopeAgent::builder("http://localhost:4040", "example.multithread")
        .backend(Pprof::new(PprofConfig::new(100)))
        .build()?;

    // Start Agent
    agent.start()?;

    let handle_1 = thread::spawn(|| {
        hash_rounds1(300_000);
    });

    let handle_2 = thread::spawn(|| {
        hash_rounds2(500_000);
    });

    // Wait for the threads to complete
    handle_1.join().unwrap();
    handle_2.join().unwrap();

    // Stop Agent
    agent.stop()?;

    Ok(())
}
