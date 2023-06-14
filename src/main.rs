pub mod pdh_helper;

use std::env;

use windows::Win32::System::Performance::PdhCloseLog;

use crate::pdh_helper::{
    bind_input_logfiles, get_perflog_summary,
};

fn main() {
    env::set_var("RUST_BACKTRACE", "1");

    //let args: Vec<String> = env::args().collect();
    //if args.len() < 2 {
    //    println!("Usage: {} <glob pattern>", args[0]);
    //    return;
    //}

    //let glob_pattern = &args[1];

    let glob_pattern = "C:\\Users\\bill\\Downloads\\*0612*.blg";

    let mut files: Vec<String> = glob::glob(glob_pattern)
        .expect("Failed to read glob pattern")
        .map(|x| x.unwrap().display().to_string())
        .collect();

    files.sort_by(|a, b| {
        let a_modified = std::fs::metadata(&a).unwrap().modified().unwrap();
        let b_modified = std::fs::metadata(&b).unwrap().modified().unwrap();
        a_modified.cmp(&b_modified)
    });

    println!("Found {} files.", &files.len());

    if files.len() == 0 {
        return;
    }

    for file in &files {
        println!("  {}", file);
    }

    let hdatasource = bind_input_logfiles(files);

    let summary = get_perflog_summary(hdatasource);

    for machine in summary.machines {
        println!("Machine: {}", machine.name);

        for object in machine.objects {
            println!("  {}", object.name);
            println!("    Counters:");
            for counter in object.counters {
                println!("      {}", counter);
            }

            println!("    Instances:");
            for instance in object.instances {
                println!("      {}", instance);
            }
        }
    }

    println!("Time range: {} - {}", summary.start_time, summary.end_time);

    unsafe { PdhCloseLog(hdatasource, 0) };
}
