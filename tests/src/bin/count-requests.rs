use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, BufRead, Write},
};

use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use statrs::statistics::{Data, OrderStatistics};

#[derive(Debug, Clone, Deserialize)]
struct EnvVar {
    time_zone_seconds: i32,
}

const ALLOWED_EXTENSION: &str = "log";

fn process_stats(
    file_name: &str,
    out_file_name: &str,
    time_zone_seconds: i32,
) -> std::io::Result<()> {
    let file = File::open(file_name)?;
    let reader = io::BufReader::new(file);

    let mut count_map: HashMap<String, usize> = HashMap::new();

    for line in reader.lines().map_while(Result::ok) {
        if let Some(timestamp) = line.split_whitespace().next() {
            let time_key = timestamp.trim_start_matches('[').to_string();
            *count_map.entry(time_key).or_insert(0) += 1;
        }
    }

    let mut sorted_keys: Vec<_> = count_map
        .keys()
        .filter(|input| {
            let re = regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z").unwrap();
            re.is_match(input)
        })
        .cloned()
        .collect();
    sorted_keys.sort();
    for key in sorted_keys.iter() {
        if let Some(count) = count_map.get(key) {
            log::debug!("{}: {}", key, count);
        }
    }

    let row_duration = 300000;

    let min_date = sorted_keys.first().unwrap();
    let max_date = sorted_keys.last().unwrap();
    log::debug!("min_date: {}", min_date);
    log::debug!("max_date: {}", max_date);

    let min_data_timestamp = get_timestamp(min_date);
    log::debug!("min_data_timestamp: {}", min_data_timestamp);
    let min_row = min_data_timestamp / row_duration * row_duration;
    log::debug!("min_row: {}", min_row);

    let max_data_timestamp = get_timestamp(max_date);
    log::debug!("max_data_timestamp: {}", max_data_timestamp);
    let max_row = (max_data_timestamp + row_duration - 1) / row_duration * row_duration;
    log::debug!("max_row: {}", max_row);

    let mut current_row = min_row;
    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
    for key in sorted_keys.iter() {
        let key_row = get_timestamp(key);
        if key_row >= current_row + row_duration {
            current_row += row_duration;
            groups.entry(current_row).or_default();
        }
        let count = *count_map.get(key).unwrap_or(&0);
        groups.entry(current_row).or_default().push(count);
    }

    let mut sorted_keys: Vec<_> = groups.keys().cloned().collect();
    sorted_keys.sort();

    let mut file = File::create(out_file_name)?;
    writeln!(&mut file, "datetime,p99,p95,p50,sum")?;
    for key in sorted_keys.iter() {
        if let Some(count) = groups.get(key) {
            let datetime = DateTime::from_timestamp(key / 1000, 0).unwrap();
            let timezone_offset =
                FixedOffset::east_opt(time_zone_seconds).expect("invalid timezone");
            let datetime_with_timezone = datetime.with_timezone(&timezone_offset);
            let mut data = Data::new(count.iter().map(|x| *x as f64).collect::<Vec<_>>());
            let sum = count.iter().sum::<usize>();
            let p50 = data.percentile(50);
            let p95 = data.percentile(95);
            let p99 = data.percentile(99);
            writeln!(
                &mut file,
                "{},{},{},{},{}",
                datetime_with_timezone.to_rfc3339(),
                p99,
                p95,
                p50,
                sum
            )?;
        }
    }

    Ok(())
}

fn get_timestamp(date_str: &str) -> i64 {
    let date = DateTime::parse_from_rfc3339(date_str).unwrap();
    date.timestamp_millis()
}

fn process_file(args: Vec<String>, time_zone_seconds: i32) -> std::io::Result<()> {
    let file_name = &args[2];
    let file_name_extension = file_name.split('.').last().unwrap();
    let file_name_without_extension =
        file_name.trim_end_matches(&format!(".{}", file_name_extension));
    let default_out_file_name = format!("{}.csv", file_name_without_extension);
    let out_file_name = &args.get(3).unwrap_or(&default_out_file_name);

    process_stats(file_name, out_file_name, time_zone_seconds)
}

fn process_dir(args: Vec<String>, time_zone_seconds: i32) -> std::io::Result<()> {
    let dir_name = &args[2];

    for entry in std::fs::read_dir(dir_name)? {
        let entry = entry?;
        let path = entry.path();
        log::debug!("path: {}", &path.to_string_lossy());
        let file_name = path.to_str().unwrap();
        let file_name_extension = file_name.split('.').last().unwrap();
        if file_name_extension != ALLOWED_EXTENSION {
            continue;
        }
        let file_name_without_extension =
            file_name.trim_end_matches(&format!(".{}", ALLOWED_EXTENSION));
        let default_out_file_name = format!("{}.csv", file_name_without_extension);
        let out_file_name = args.get(3).unwrap_or(&default_out_file_name);
        log::debug!("out_file_name: {}", &out_file_name);

        process_stats(file_name, out_file_name, time_zone_seconds)?;
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv()?;
    dotenv::from_path("../cli/.env")?;
    let config = envy::from_env::<EnvVar>().unwrap();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let time_zone_seconds: i32 = config.time_zone_seconds;
    log::info!("time_zone_hours: {}", time_zone_seconds as f64 / 3600.0);

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file_type> <log_file> [out_file]", args[0]);
        return Ok(());
    }

    if args[1] == "file" {
        process_file(args, time_zone_seconds)?;
    } else if args[1] == "dir" {
        process_dir(args, time_zone_seconds)?;
    } else {
        eprintln!("Usage: {} file <log_file> [out_file]", args[0]);
        eprintln!("Usage: {} dir <log_dir>", args[0]);
        return Ok(());
    }

    Ok(())
}
