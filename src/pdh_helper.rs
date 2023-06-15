use std::{collections::HashMap, time::Duration};

use time::{macros::datetime, PrimitiveDateTime};
use windows::{
    core::{HSTRING, PWSTR},
    Win32::System::Performance::{
        PdhAddCounterW, PdhBindInputDataSourceW, PdhCollectQueryDataWithTime, PdhEnumMachinesHW,
        PdhEnumObjectItemsHW, PdhEnumObjectsHW, PdhGetDataSourceTimeRangeH,
        PdhGetFormattedCounterValue, PdhOpenQueryH, PDH_CSTATUS_NO_OBJECT, PDH_FMT_COUNTERVALUE,
        PDH_FMT_LARGE, PDH_INVALID_DATA, PDH_MORE_DATA, PDH_TIME_INFO, PERF_DETAIL_WIZARD,
    },
};

pub enum CounterValueWithTime {
    Long(PrimitiveDateTime, i32),
    Double(PrimitiveDateTime, f64),
    Large(PrimitiveDateTime, i64),
}

pub struct PerfLogSummary {
    pub machines: Vec<MachineSummary>,
    pub start_time: time::PrimitiveDateTime,
    pub end_time: time::PrimitiveDateTime,
}

impl PerfLogSummary {
    pub fn print_hierarchy(&self) {
        for machine in &self.machines {
            println!("Machine: {}", machine.name);

            for object in &machine.objects {
                println!("  {}", object.name);
                println!("    Counters:");
                for counter in &object.counters {
                    println!("      {}", counter);
                }

                println!("    Instances:");
                for instance in &object.instances {
                    println!("      {}", instance);
                }
            }
        }
    }

    pub fn get_all_counters(&self) -> Vec<String> {
        let mut all_counters = Vec::new();
        for machine in &self.machines {
            for object in &machine.objects {
                for instance in &object.instances {
                    for counter in &object.counters {
                        all_counters.push(format!(
                            "{}\\{}({})\\{}",
                            machine.name, object.name, instance, counter
                        ));
                    }
                }
            }
        }

        all_counters
    }
}

pub struct MachineSummary {
    pub name: String,
    pub objects: Vec<ObjectSummary>,
}

pub struct ObjectSummary {
    pub name: String,
    pub counters: Vec<String>,
    pub instances: Vec<String>,
}

pub fn get_perflog_summary(hdatasource: isize) -> PerfLogSummary {
    let mut machines = Vec::new();

    let machine_names = enum_machines(hdatasource);

    for machine in machine_names {
        let object_names = enum_objects(&machine, hdatasource);

        let mut objects = Vec::new();

        for object in object_names {
            let (counter_names, instance_names) =
                match enum_object_items(&machine, &object, hdatasource) {
                    Some(value) => value,
                    None => continue,
                };

            let object_summary = ObjectSummary {
                name: object,
                counters: counter_names,
                instances: instance_names,
            };

            objects.push(object_summary);
        }

        machines.push(MachineSummary {
            name: machine,
            objects,
        });
    }

    let (start_time, end_time) = get_time_range(hdatasource);

    let summary = PerfLogSummary {
        machines,
        start_time,
        end_time,
    };

    summary
}

pub fn get_time_range(hdatasource: isize) -> (time::PrimitiveDateTime, time::PrimitiveDateTime) {
    let mut pdwnumentries = 0;
    let mut pinfo = PDH_TIME_INFO {
        StartTime: 0,
        EndTime: 0,
        SampleCount: 0,
    };
    let mut pdwbuffersize = 24;
    let pdhstatus = unsafe {
        PdhGetDataSourceTimeRangeH(
            hdatasource,
            &mut pdwnumentries,
            &mut pinfo,
            &mut pdwbuffersize,
        )
    };

    if pdhstatus != 0 {
        panic!("Failed to get time range: {:#x}", pdhstatus);
    }

    let start_time = get_time_from_filetime(pinfo.StartTime);
    let end_time = get_time_from_filetime(pinfo.EndTime);
    (start_time, end_time)
}

pub fn get_time_from_filetime(filetime: i64) -> time::PrimitiveDateTime {
    let filetime_basedate = datetime!(1601-01-01 00:00:00);
    let nanos = Duration::from_nanos(filetime as u64 * 100);
    filetime_basedate + nanos
}

pub fn enum_object_items(
    machine: &String,
    object: &String,
    hdatasource: isize,
) -> Option<(Vec<String>, Vec<String>)> {
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

pub fn enum_objects(machine: &String, hdatasource: isize) -> Vec<String> {
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

pub fn enum_machines(hdatasource: isize) -> Vec<String> {
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

pub fn bind_input_logfiles(files: Vec<String>) -> isize {
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

pub fn read_counter_values(
    hdatasource: isize,
    counters_to_read: &Vec<&String>,
) -> HashMap<String, Vec<CounterValueWithTime>> {
    let mut counter_data = HashMap::<String, Vec<CounterValueWithTime>>::new();

    let mut phquery: isize = isize::default();
    let pdhstatus = unsafe { PdhOpenQueryH(hdatasource, 0, &mut phquery) };

    if pdhstatus != 0 {
        panic!("Failed to open query: {:#x}", pdhstatus);
    }

    let mut counter_handles = HashMap::<String, isize>::new();

    for counter in counters_to_read {
        let counter_path = HSTRING::from(*counter);
        let mut phcounter: isize = isize::default();
        let pdhstatus = unsafe { PdhAddCounterW(phquery, &counter_path, 0, &mut phcounter) };

        if pdhstatus != 0 {
            panic!("Failed to add counter: {:#x}", pdhstatus);
        }

        counter_handles.insert(counter.to_string(), phcounter);
        counter_data.insert(counter.to_string(), Vec::<CounterValueWithTime>::new());
    }

    loop {
        let mut filetime: i64 = 0;
        let pdhstatus = unsafe { PdhCollectQueryDataWithTime(phquery, &mut filetime) };

        if pdhstatus != 0 {
            break;
        }

        let time = get_time_from_filetime(filetime);

        for (counter_name, h_counter) in &counter_handles {
            let mut pvalue = PDH_FMT_COUNTERVALUE::default();
            let pdhstatus = unsafe {
                PdhGetFormattedCounterValue(*h_counter, PDH_FMT_LARGE, None, &mut pvalue)
            };

            match pdhstatus {
                PDH_INVALID_DATA => println!("{} {}: {}", time, counter_name, "Invalid data"),

                0 => match pvalue.CStatus {
                    0 => unsafe {
                        let cv = CounterValueWithTime::Large(time, pvalue.Anonymous.largeValue);
                        counter_data
                            .get_mut(counter_name)
                            .expect("Key not found")
                            .push(cv);
                    },
                    _ => {
                        println!(
                            "{} {}: Unexpected CStatus {}",
                            time, counter_name, pvalue.CStatus
                        );
                    }
                },

                _ => {
                    panic!("Failed to get counter value: {:#x}", pdhstatus);
                }
            }
        }
    }
    counter_data
}
