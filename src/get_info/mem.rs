use crate::data_struct::{Disk, Ram, Swap};
use log::trace;
use std::collections::HashSet;
use std::sync::OnceLock;
use sysinfo::{Disks, System};

#[derive(Debug)]
pub struct MemDiskTotalInfoWithOutUsage {
    pub mem: u64,
    pub swap: u64,
    pub disk: u64,
}

pub fn mem_info_without_usage(sysinfo_sys: &System) -> MemDiskTotalInfoWithOutUsage {
    let mem_total = sysinfo_sys.total_memory();
    let swap_total = sysinfo_sys.total_swap();

    let disks = Disks::new_with_refreshed_list();
    let disk_list = filter_disks(&disks);
    let mut all_disk_space: u64 = 0;
    for disk in &disk_list {
        all_disk_space += disk.total_space();
    }

    let info = MemDiskTotalInfoWithOutUsage {
        mem: mem_total,
        swap: swap_total,
        disk: all_disk_space,
    };

    trace!("MEM DISK INFO WITH OUT USAGE successfully retrieved: {info:?}");

    info
}

pub fn realtime_mem(sysinfo_sys: &System) -> Ram {
    let ram = Ram {
        used: sysinfo_sys.total_memory() - sysinfo_sys.available_memory(),
        total: sysinfo_sys.total_memory(),
    };
    trace!("REALTIME MEM successfully retrieved: {ram:?}");
    ram
}

pub fn realtime_swap(sysinfo_sys: &System) -> Swap {
    let swap = Swap {
        used: sysinfo_sys.used_swap(),
        total: sysinfo_sys.total_swap(),
    };
    trace!("REALTIME SWAP successfully retrieved: {swap:?}");
    swap
}

pub fn realtime_disk(disk: &Disks) -> Disk {
    let mut used_disk: u64 = 0;
    let mut total_disk: u64 = 0;
    let disk_list = filter_disks(disk);
    for disk in disk_list {
        trace!("FILTERED DISK: {disk:?}");
        used_disk += disk.total_space() - disk.available_space();
        total_disk += disk.total_space();
    }

    let disk_info = Disk { 
        used: used_disk,
        total: total_disk,
    };
    trace!("REALTIME DISK successfully retrieved: {disk_info:?}");
    disk_info
}

fn get_allowed_filesystems() -> &'static HashSet<&'static str> {
    static ALLOWED_FS: OnceLock<HashSet<&str>> = OnceLock::new();
    ALLOWED_FS.get_or_init(|| {
        [
            "apfs",
            "ext4",
            "ext3",
            "ext2",
            "f2fs",
            "reiserfs",
            "jfs",
            "btrfs",
            "fuseblk",
            "zfs",
            "simfs",
            "ntfs",
            "fat32",
            "exfat",
            "xfs",
            "fuse.rclone",
            "ubifs",
        ]
        .iter()
        .cloned()
        .collect()
    })
}

fn get_exclude_keywords() -> &'static HashSet<&'static str> {
    static EXCLUDE_KEYWORDS: OnceLock<HashSet<&str>> = OnceLock::new();
    EXCLUDE_KEYWORDS.get_or_init(|| {
        [
            "/snap",
            "/var/lib/docker",
            "/var/lib/lxcfs",
            "/run/user",
            "/tmp",
            "/dev",
            "/sys",
            "/proc",
            "/boot",
            "/lost+found",
            "/nix/store",
            "/var/log.hdd",
        ]
        .iter()
        .cloned()
        .collect()
    })
}

pub fn filter_disks(disks: &Disks) -> Vec<&sysinfo::Disk> {
    let allowed_fs = get_allowed_filesystems();
    let exclude_keywords = get_exclude_keywords();

    let mut unique_disks = Vec::new();
    let mut seen_devices = HashSet::new();

    for disk in disks.iter() {
        // Filter by filesystem type
        let fs = disk.file_system().to_string_lossy();
        if !allowed_fs.contains(fs.as_ref()) {
            continue;
        }

        // Filter by mount point
        let mount_point = disk.mount_point().to_string_lossy();
        if exclude_keywords
            .iter()
            .any(|keyword| mount_point.contains(keyword))
        {
            continue;
        }

        // Deduplicate by device name
        let name = disk.name().to_string_lossy().into_owned();
        if seen_devices.insert(name) {
            unique_disks.push(disk);
        }
    }

    unique_disks
}
