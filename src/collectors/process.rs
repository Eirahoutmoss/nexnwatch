use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub parent_pid: Option<u32>,
    /// Başlangıç zamanı (UNIX sn). PID yeniden kullanımını ayırt etmek için.
    pub start_time: u64,
    pub cpu: f32,
    pub memory: u64,
}

pub fn snapshot(system: &mut System) -> Vec<ProcessInfo> {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory(),
    );

    system
        .processes()
        .iter()
        .map(|(pid, process)| ProcessInfo {
            pid: pid.as_u32(),
            name: process.name().to_string_lossy().into_owned(),
            parent_pid: process.parent().map(Pid::as_u32),
            start_time: process.start_time(),
            cpu: process.cpu_usage(),
            memory: process.memory(),
        })
        .collect()
}
