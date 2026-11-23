//! Build a GAC1 boot catalog from the persistent GUA1 directory and ATID files.
//!
//! Usage:
//! cargo run -p globus-identity --example build-boot-catalog -- \\
//!   <account-directory.gua> <sys.accounts> <user-id>=<credential.atid> [...]

use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use globus_identity::{build_boot_catalog, AccountCredential, FileAccountDirectory, UserId};

fn main() {
    let mut args = env::args().skip(1);
    let directory_path = args.next().unwrap_or_else(|| usage());
    let output_path = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let directory = FileAccountDirectory::open(directory_path);
    let mut credentials = Vec::new();

    for argument in args {
        let (user_id, path) = argument
            .split_once('=')
            .unwrap_or_else(|| panic!("credential argument must be <user-id>=<path>"));
        let user_id = UserId::new(user_id.to_owned()).unwrap_or_else(|_| panic!("invalid UserId"));
        let record = fs::read(path).unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
        credentials.push(AccountCredential { user_id, record });
    }

    let catalog = build_boot_catalog(&directory, &credentials)
        .unwrap_or_else(|error| panic!("boot catalog rejected (fail-closed): {error:?}"));
    let parent = output_path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    fs::create_dir_all(parent).expect("cannot create output directory");
    let mut temporary = output_path.as_os_str().to_os_string();
    temporary.push(format!(".tmp.{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)
        .expect("temporary catalog file already exists or cannot be created");
    file.write_all(&catalog).expect("cannot write catalog");
    file.sync_all().expect("cannot sync catalog");
    drop(file);
    fs::rename(&temporary, &output_path).expect("cannot atomically publish catalog");
    if let Ok(directory) = fs::File::open(parent) {
        directory.sync_all().expect("cannot sync catalog directory");
    }
    println!("GAC1 boot catalog: {} bytes -> {}", catalog.len(), output_path.display());
}

fn usage() -> ! {
    eprintln!("usage: build-boot-catalog <account-directory.gua> <sys.accounts> <user-id>=<credential.atid> [...]");
    std::process::exit(2);
}
