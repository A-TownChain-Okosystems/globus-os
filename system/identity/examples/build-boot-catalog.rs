//! Package an exported GUA1 account-directory snapshot and ATID records for the boot Ramdisk.
//!
//! The GUA1 input must be an ephemeral export from the canonical encrypted
//! AccountStateRepository. Do not use a separate persistent FileAccountDirectory
//! as a production source of truth.
//!
//! Usage:
//! cargo run -p globus-identity --example build-boot-catalog -- \\
//!   <account-directory.gua> <sys.accounts> <sys.credentials> \\
//!   <user-id>=<credential.atid> [...] | --gcr1=<credential-catalog.gcr>

use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use globus_identity::{AccountCredential, AccountDirectorySnapshot, UserId, build_boot_catalog};

fn main() {
    let mut args = env::args().skip(1);
    let directory_path = args.next().unwrap_or_else(|| usage());
    let accounts_path = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let credentials_path = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let directory_bytes = fs::read(&directory_path).unwrap_or_else(|error| {
        panic!("cannot read exported GUA1 snapshot {directory_path}: {error}")
    });
    let directory = AccountDirectorySnapshot::from_bytes(&directory_bytes)
        .unwrap_or_else(|error| panic!("exported GUA1 snapshot rejected: {error:?}"));
    let remaining: Vec<String> = args.collect();
    let images = if remaining.len() == 1 && remaining[0].starts_with("--gcr1=") {
        let catalog_path = remaining[0].trim_start_matches("--gcr1=");
        let credential_bytes = fs::read(catalog_path).unwrap_or_else(|error| {
            panic!("cannot read exported GCR1 snapshot {catalog_path}: {error}")
        });
        globus_identity::build_boot_catalog_from_snapshots(&directory_bytes, &credential_bytes)
            .unwrap_or_else(|error| panic!("exported GUA1/GCR1 snapshots rejected: {error:?}"))
    } else {
        let mut credentials = Vec::new();
        for argument in remaining {
        let (user_id, path) = argument
            .split_once('=')
            .unwrap_or_else(|| panic!("credential argument must be <user-id>=<path>"));
        let user_id = UserId::new(user_id.to_owned()).unwrap_or_else(|_| panic!("invalid UserId"));
        let record = fs::read(path).unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
        credentials.push(AccountCredential { user_id, record });
    }

        build_boot_catalog(&directory, &credentials)
            .unwrap_or_else(|error| panic!("boot catalog rejected (fail-closed): {error:?}"))
    };
    write_atomic(&accounts_path, &images.accounts);
    write_atomic(&credentials_path, &images.credentials);
    println!(
        "GUA1 sys.accounts: {} bytes -> {}",
        images.accounts.len(),
        accounts_path.display()
    );
    println!(
        "GCR1 sys.credentials: {} bytes -> {}",
        images.credentials.len(),
        credentials_path.display()
    );
}

fn write_atomic(path: &Path, bytes: &[u8]) {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).expect("cannot create output directory");
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(format!(".tmp.{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .expect("temporary output exists or cannot be created");
    file.write_all(bytes).expect("cannot write output");
    file.sync_all().expect("cannot sync output");
    drop(file);
    fs::rename(&temporary, path).expect("cannot atomically publish output");
    if let Ok(directory) = fs::File::open(parent) {
        directory.sync_all().expect("cannot sync output directory");
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: build-boot-catalog <account-directory.gua> <sys.accounts> <sys.credentials> <user-id>=<credential.atid> [...]"
    );
    std::process::exit(2);
}
