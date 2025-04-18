use std::env;
use std::fs::{read_dir, File};
use std::io::{Result, Write};

fn main() {
    let target_path = env::var("USER_TARGET_PATH")
        .unwrap_or_else(|_| "../user/target/loongarch64-unknown-none/release/".to_string());
    println!("cargo:rerun-if-changed=../user/src/");
    println!("cargo:rerun-if-changed={}", target_path);
    insert_app_data(&target_path).unwrap();
}

// static TARGET_PATH: &str = "../user/target/loongarch64-unknown-none/release/";

fn insert_app_data(target_path: &str) -> Result<()> {
    let mut f = File::create("src/link_app.S").unwrap();
    let mut apps: Vec<_> = read_dir("../user/src/bin")
        .unwrap()
        .into_iter()
        .filter(|dir_entry| dir_entry.as_ref().unwrap().file_type().unwrap().is_file())
        .map(|dir_entry| {
            let mut name_with_ext = dir_entry.unwrap().file_name().into_string().unwrap();
            name_with_ext.drain(name_with_ext.find('.').unwrap()..name_with_ext.len());
            name_with_ext
        })
        .collect();
    apps.sort();

    writeln!(
        f,
        r#"
    .align 3
    .section .data
    .global _num_app
_num_app:
    .quad {}"#,
        apps.len()
    )?;

    for i in 0..apps.len() {
        writeln!(f, r#"    .quad app_{}_start"#, i)?;
    }
    writeln!(f, r#"    .quad app_{}_end"#, apps.len() - 1)?;

    writeln!(
        f,
        r#"
    .global _app_names
_app_names:"#
    )?;
    for app in apps.iter() {
        writeln!(f, r#"    .string "{}""#, app)?;
    }

    for (idx, app) in apps.iter().enumerate() {
        println!("app_{}: {}", idx, app);
        writeln!(
            f,
            r#"
    .section .data
    .global app_{0}_start
    .global app_{0}_end
    .align 3
app_{0}_start:
    .incbin "{2}{1}"
app_{0}_end:"#,
            idx, app, target_path
        )?;
    }
    Ok(())
}
