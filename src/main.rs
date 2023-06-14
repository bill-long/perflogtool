use std::{env, time::Duration};

use time::macros::datetime;
use windows::{
    core::{HSTRING, PWSTR},
    Win32::{System::{Performance::{
        PdhBindInputDataSourceW, PdhEnumMachinesHW, PdhEnumObjectItemsHW, PdhEnumObjectsHW,
        PDH_CSTATUS_NO_OBJECT, PDH_MORE_DATA, PERF_DETAIL_WIZARD, PdhGetDataSourceTimeRangeH, PDH_TIME_INFO, PdhCloseLog,
      }}  }
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

    let machine_names = enum_machines(hdatasource);

    for machine in machine_names {
        println!("Machine: {}", machine);

        let object_names = enum_objects(&machine, hdatasource);

        for object in object_names {
            println!("  {}", object);

            let (counter_names, instance_names) = match enum_object_items(&machine, object, hdatasource) {
                Some(value) => value,
                None => continue,
            };

            println!("    Counters:");
            for counter in counter_names {
                println!("      {}", counter);
            }

            println!("    Instances:");
            for instance in instance_names {
                println!("      {}", instance);
            }
        }
    }

    let (start_time, end_time) = get_time_range(hdatasource);

    println!("Time range: {} - {}", start_time, end_time);

    unsafe { PdhCloseLog(hdatasource, 0) };
}

fn get_time_range(hdatasource: isize) -> (time::PrimitiveDateTime, time::PrimitiveDateTime) {
    let mut pdwnumentries = 0;
    let mut pinfo = PDH_TIME_INFO{StartTime: 0, EndTime: 0, SampleCount: 0};
    let mut pdwbuffersize = 24;
    let pdhstatus = unsafe { PdhGetDataSourceTimeRangeH(hdatasource, &mut pdwnumentries, &mut pinfo, &mut pdwbuffersize) };

    if pdhstatus != 0 {
        panic!("Failed to get time range: {:#x}", pdhstatus);
    }

    let filetime_basedate = datetime!(1601-01-01 00:00:00);
    let start_nanos = Duration::from_nanos(pinfo.StartTime as u64 * 100);
    let start_time = filetime_basedate + start_nanos;
    let end_nanos = Duration::from_nanos(pinfo.EndTime as u64 * 100);
    let end_time = filetime_basedate + end_nanos;
    (start_time, end_time)
}

fn enum_object_items(machine: &String, object: String, hdatasource: isize) -> Option<(Vec<String>, Vec<String>)> {
    let szmachinename = HSTRING::from(machine);
    let szobjectname = HSTRING::from(object);
    let mszcounterlist = PWSTR::null();
    let mut pcchcounterlistlength = 0;
    let mszinstancelist = PWSTR::null();
    let mut pcchinstancelistlength = 0;
    let pdhstatus = unsafe {
        PdhEnumObjectItemsHW(
            hdatasource,
            &szmachinename,
            &szobjectname,
            mszcounterlist,
            &mut pcchcounterlistlength,
            mszinstancelist,
            &mut pcchinstancelistlength,
            PERF_DETAIL_WIZARD,
            0,
        )
    };

    if pdhstatus == PDH_CSTATUS_NO_OBJECT {
        // This happens due to invalid object names in the file. Skip it.
        return None;
    }

    if pdhstatus != PDH_MORE_DATA {
        panic!(
            "Failed to get buffer size to enum counters: {:#x}",
            pdhstatus
        );
    }
    
    let mut counterlist = vec![0u16; pcchcounterlistlength as usize];
    let mszcounterlist: PWSTR = PWSTR(counterlist.as_mut_ptr());
    let mut instancelist = vec![0u16; pcchinstancelistlength as usize];
    let mszinstancelist: PWSTR = PWSTR(instancelist.as_mut_ptr());
    let pdhstatus = unsafe {
        PdhEnumObjectItemsHW(
            hdatasource,
            &szmachinename,
            &szobjectname,
            mszcounterlist,
            &mut pcchcounterlistlength,
            mszinstancelist,
            &mut pcchinstancelistlength,
            PERF_DETAIL_WIZARD,
            0,
        )
    };

    if pdhstatus != 0 {
        panic!("Failed to enum counters: {:#x}", pdhstatus);
    }

    let counter_names = get_strings_from_pwstr(&mszcounterlist, pcchcounterlistlength);
    let instance_names = get_strings_from_pwstr(&mszinstancelist, pcchinstancelistlength);

    Some((counter_names, instance_names))
}

fn enum_objects(machine: &String, hdatasource: isize) -> Vec<String> {
    let szmachinename = HSTRING::from(machine);

    let mut cb_buffer = 0;
    let lp_buffer = PWSTR::null();

    let pdhstatus = unsafe {
        PdhEnumObjectsHW(
            hdatasource,
            &szmachinename,
            lp_buffer,
            &mut cb_buffer,
            PERF_DETAIL_WIZARD,
            false,
        )
    };

    if pdhstatus != PDH_MORE_DATA {
        panic!(
            "Failed to get buffer size to enum objects: {:#x}",
            pdhstatus
        );
    }

    let mut real_object_list = vec![0u16; cb_buffer as usize];
    let lp_buffer: PWSTR = PWSTR(real_object_list.as_mut_ptr());
    let pdhstatus = unsafe {
        PdhEnumObjectsHW(
            hdatasource,
            &szmachinename,
            lp_buffer,
            &mut cb_buffer,
            PERF_DETAIL_WIZARD,
            false,
        )
    };

    if pdhstatus != 0 {
        panic!("Failed to enum objects: {:#x}", pdhstatus);
    }

    let object_names = get_strings_from_pwstr(&lp_buffer, cb_buffer);
    object_names
}

fn enum_machines(hdatasource: isize) -> Vec<String> {
    let mut buffer_size = 0;
    let machine_list = PWSTR::null();
    let pdhstatus = unsafe { PdhEnumMachinesHW(hdatasource, machine_list, &mut buffer_size) };

    if pdhstatus != PDH_MORE_DATA {
        panic!(
            "Failed to get buffer size to enum machines: {:#x}",
            pdhstatus
        );
    }

    let mut real_machine_list = vec![0u16; buffer_size as usize];
    let lp_buffer: PWSTR = PWSTR(real_machine_list.as_mut_ptr());
    let pdhstatus = unsafe { PdhEnumMachinesHW(hdatasource, lp_buffer, &mut buffer_size) };

    if pdhstatus != 0 {
        panic!("Failed to enum machines: {:#x}", pdhstatus);
    }

    let machine_names = get_strings_from_pwstr(&lp_buffer, buffer_size);
    machine_names
}

fn bind_input_logfiles(files: Vec<String>) -> isize {
    let mut file_list = String::new();
    for file in files {
        file_list.push_str(&file);
        file_list.push('\0');
    }

    file_list.push('\0');

    let file = HSTRING::from(&file_list);

    let mut hdatasource: isize = isize::default();
    let pdhstatus = unsafe { PdhBindInputDataSourceW(&mut hdatasource, &file) };

    if pdhstatus != 0 {
        panic!("Failed to bind to log files: {:#x}", pdhstatus);
    }

    hdatasource
}

fn get_strings_from_pwstr(object_list: &PWSTR, buffer_size: u32) -> Vec<String> {
    let object_list_ptr = object_list.as_ptr();
    let slice = unsafe { std::slice::from_raw_parts(object_list_ptr, buffer_size as usize) };

    let mut strings = Vec::<String>::new();
    let current_string = String::from_utf16(slice).unwrap();
    let split = current_string.split('\0');

    for s in split {
        strings.push(s.to_string());
    }

    strings.pop();
    strings.pop();

    strings
}
