use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, BufRead},
};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <log_file>", args[0]);
        return Ok(());
    }
    let file_name = &args[1];

    let file = File::open(file_name)?;
    let reader = io::BufReader::new(file);

    let mut count_map: HashMap<String, usize> = HashMap::new();

    for line in reader.lines().map_while(Result::ok) {
        if let Some(timestamp) = line.split_whitespace().next() {
            let time_key = timestamp.trim_start_matches('[').to_string();
            *count_map.entry(time_key).or_insert(0) += 1;
        }
    }

    let mut sorted_keys: Vec<_> = count_map.keys().collect();
    sorted_keys.sort();
    for key in sorted_keys {
        if let Some(count) = count_map.get(key) {
            println!("{}: {}", key, count);
        }
    }

    Ok(())
}
