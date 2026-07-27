#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use crate::calc::{ConfigSchema, process};
use std::fs::File;

mod calc;

fn main() {
    println!("Calculating...");
    let start = std::time::Instant::now();
    let file = File::open("config.yml").expect("unable to open config.yml");
    let config: ConfigSchema = yaml_serde::from_reader(file).expect("unable to read config.yml");
    let result = process(config);
    println!("Done in {}ms", start.elapsed().as_millis());
    println!("{result}");
}
