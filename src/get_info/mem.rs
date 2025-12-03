use crate::data_struct::{Disk, Ram, Swap};
use log::trace;
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

    trace!("MEM DISK INFO WITH OUT USAGE 获取成功: {info:?}");

    info
}

pub fn realtime_mem(sysinfo_sys: &System) -> Ram {
    let ram = Ram {
        used: sysinfo_sys.total_memory() - sysinfo_sys.available_memory(),
    };
    trace!("REALTIME MEM 获取成功: {ram:?}");
    ram
}

pub fn realtime_swap(sysinfo_sys: &System) -> Swap {
    let swap = Swap {
        used: sysinfo_sys.used_swap(),
    };
    trace!("REALTIME SWAP 获取成功: {swap:?}");
    swap
}

pub fn realtime_disk(disk: &Disks) -> Disk {
    let mut used_disk: u64 = 0;
    let disk_list = filter_disks(disk);
    for disk in disk_list {
        trace!("FILTER DISK: {disk:?}");
        used_disk += disk.total_space() - disk.available_space();
    }

    let disk_info = Disk { used: used_disk };
    trace!("REALTIME DISK 获取成功: {disk_info:?}");
    disk_info
}

pub fn filter_disks(disks: &Disks) -> Vec<&sysinfo::Disk> {
    let allowed_fs = [
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
    ];

    let filtered_fs: Vec<&sysinfo::Disk> = disks
        .iter() // 返回 &Disk
        .filter(|disk| {
            let fs = disk.file_system().to_string_lossy();
            allowed_fs.contains(&fs.as_ref())
        })
        .collect();

    let exclude_keywords = [
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
    ];

    let filtered_special_mount_point: Vec<&sysinfo::Disk> = filtered_fs
        .into_iter()
        .filter(|disk| {
            let mount_point = disk.mount_point().to_string_lossy();
            !exclude_keywords
                .iter()
                .any(|keyword| mount_point.contains(keyword))
        })
        .collect();

    filtered_special_mount_point
}
