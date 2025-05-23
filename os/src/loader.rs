//! Loading user applications into memory

/// Get the total number of applications.
use alloc::vec::Vec;
use lazy_static::*;

///get app number
pub fn get_num_app() -> usize {
    extern "C" {
        fn _num_app();
    }
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}
/// get applications data
pub fn get_app_data(app_id: usize) -> &'static [u8] {
    extern "C" {
        fn _num_app();
    }
    let num_app_ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    assert!(app_id < num_app);
    unsafe {
        core::slice::from_raw_parts(
            app_start[app_id] as *const u8,
            app_start[app_id + 1] - app_start[app_id],
        )
    }
}

lazy_static! {
    ///All of app's name
    static ref APP_NAMES: Vec<&'static str> = {
        let num_app = get_num_app();
        extern "C" {
            fn _app_names();
        }
        let mut start = _app_names as usize as *const u8;
        let mut v = Vec::new();
        unsafe {
            for _ in 0..num_app {
                let mut end = start;
                while end.read_volatile() != b'\0' {
                    end = end.add(1);
                }
                let slice = core::slice::from_raw_parts(start, end as usize - start as usize);
                let str = core::str::from_utf8(slice).unwrap();
                v.push(str);
                start = end.add(1);
            }
        }
        v
    };
}

#[allow(unused)]
///get app data from name
pub fn get_app_data_by_name(name: &str) -> Option<&'static [u8]> {
    let num_app = get_num_app();
    log::error!("num_app: {}", num_app);
    (0..num_app)
        .find(|&i| APP_NAMES[i] == name)
        .map(get_app_data)
}

// pub fn load_dl_interp_if_needed(elf: &ElfFile) -> Option<usize> {
//     let elf_header = elf.header;
//     let ph_count = elf_header.pt2.ph_count();

//     let mut is_dynamic_link = false;

//     // check if the elf is dynamic link
//     for i in 0..ph_count {
//         let ph = elf.program_header(i).unwrap();
//         if ph.get_type().unwrap() == xmas_elf::program::Type::Interp {
//             is_dynamic_link = true;
//             break;
//         }
//     }
//     if is_dynamic_link {
//         // load dynamic link interpreter
//         let section = elf.find_section_by_name(".interp").unwrap();
//         let mut interp = String::from_utf8(section.raw_data(&elf).to_vec()).unwrap();
//         interp = interp.trim_end_matches('\0').to_string();
//         info!("[load_dl] interp: {}", interp);
//         // load interp
//         // Todo: dynamic interpreter
//         let interp_data = get_app_data_by_name(&interp).unwrap();
//         let interp_entry = read_elf(interp_data).0;
//         return Some(interp_entry);
//     }
//     Some(0)
// }

///list all apps
pub fn list_apps() {
    // println!("/**** LINKED APPS ****");
    println!("[kernel] LINKED APPS >>>");
    for app in APP_NAMES.iter() {
        print!("{} \t", app);
    }
    println!("");
    // println!("**************/");
}
