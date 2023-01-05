#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

extern crate core;

use std::fs::File;
use crate::calc::{ConfigSchema, process};

mod calc;

fn main() {
    println!("Calculating...");
    let start = std::time::Instant::now();
    let file = File::open("config.yml").expect("unable to open config.yml");
    let config: ConfigSchema = serde_yaml::from_reader(file).expect("unable to read config.yml");
    let result = process(config);
    println!("Done in {}ms", start.elapsed().as_millis());
    println!("{}", result);
}
