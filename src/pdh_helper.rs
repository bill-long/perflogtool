use std::time::Duration;

use time::macros::datetime;
use windows::{
    core::{HSTRING, PWSTR},
    Win32::System::Performance::{
        PdhBindInputDataSourceW, PdhEnumMachinesHW, PdhEnumObjectItemsHW, PdhEnumObjectsHW,
        PdhGetDataSourceTimeRangeH, PDH_CSTATUS_NO_OBJECT, PDH_MORE_DATA, PDH_TIME_INFO,
        PERF_DETAIL_WIZARD,
    },
};

pub struct PerfLogSummary {
    pub machines: Vec<MachineSummary>,
    pub start_time: time::PrimitiveDateTime,
    pub end_time: time::PrimitiveDateTime,
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

        machines.push(MachineSummary { name: machine, objects });
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

    let filetime_basedate = datetime!(1601-01-01 00:00:00);
    let start_nanos = Duration::from_nanos(pinfo.StartTime as u64 * 100);
    let start_time = filetime_basedate + start_nanos;
    let end_nanos = Duration::from_nanos(pinfo.EndTime as u64 * 100);
    let end_time = filetime_basedate + end_nanos;
    (start_time, end_time)
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
