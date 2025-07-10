use std::{fs, io, os, path};

fn run() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <path>", args[0]);
        return;
    }
    let path = path::Path::new(&args[1]);
    if !path.exists() {
        println!("Error: {} does not exist", args[1]);
        return;
    }
    if !path.is_dir() {
        println!("Error: {} is not a directory", args[1]);
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        if file_type.is_file() {
            println!("{}", entry.path().display());
        }
    }
    println!("Hello, world!");
}
