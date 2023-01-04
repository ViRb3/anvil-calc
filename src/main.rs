extern crate core;

use std::fs::File;
use crate::calc::{ConfigSchema, process};

mod calc;

fn main() {
    println!("Preparing...");
    let file = File::open("config.yml").expect("unable to open config.yml");
    let config: ConfigSchema = serde_yaml::from_reader(file).expect("unable to read config.yml");
    process(config);
    println!("Done");
}
