use std::{
    env,
    io::BufReader,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use super::ProcessGroupCommandExt;
use serde::Deserialize;

#[derive(Deserialize)]
struct HostSnapshot {
    os: String,
    cpu: String,
    memory_total_bytes: u64,
    memory_available_bytes: u64,
    gpu: Vec<String>,
    graphics_drivers: Vec<String>,
    monitors: Vec<String>,
    controllers: Vec<String>,
    session: String,
}

pub struct Diagnostics {
    pub os: String,
    pub cpu: String,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub gpu: String,
    pub controllers: Vec<String>,
    pub session: String,
    pub dependencies: Vec<(String, bool)>,
}

const SNAPSHOT_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$os = Get-CimInstance -ClassName Win32_OperatingSystem
$cpu = (Get-CimInstance -ClassName Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Name)
if (-not $cpu) { $cpu = 'Unavailable' }
$video = @(Get-CimInstance -ClassName Win32_VideoController -ErrorAction SilentlyContinue)
$gpu = @($video | ForEach-Object { $_.Name } | Where-Object { $_ })
$graphicsDrivers = @($video | Where-Object { $_.DriverVersion } | ForEach-Object { "$($_.Name): $($_.DriverVersion)" })
$monitors = @(Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorID -ErrorAction SilentlyContinue | Where-Object { $_.Active } | ForEach-Object { -join ($_.UserFriendlyName | Where-Object { $_ -ne 0 } | ForEach-Object { [char]$_ }) } | Where-Object { $_ } | Sort-Object -Unique)
$controllers = @(Get-CimInstance -ClassName Win32_PnPEntity -Filter "PNPClass='HIDClass'" -ErrorAction SilentlyContinue | Where-Object { $_.Name -match 'Xbox|DualSense|DualShock|Wireless Controller|Gamepad' } | ForEach-Object { $_.Name } | Sort-Object -Unique)
[pscustomobject]@{
  os = "$($os.Caption) ($($os.Version))"
  cpu = $cpu
  memory_total_bytes = [uint64]$os.TotalVisibleMemorySize * 1024
  memory_available_bytes = [uint64]$os.FreePhysicalMemory * 1024
  gpu = @($gpu)
  graphics_drivers = @($graphicsDrivers)
  monitors = @($monitors)
  controllers = @($controllers)
  session = if ($env:SESSIONNAME) { $env:SESSIONNAME } else { 'Interactive desktop' }
} | ConvertTo-Json -Compress -Depth 3
"#;

const SNAPSHOT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);

fn powershell() -> Option<PathBuf> {
    let system_root = env::var_os("SystemRoot")?;
    let path = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    path.is_file().then_some(path)
}

fn snapshot_json(output: &[u8]) -> Result<HostSnapshot, String> {
    serde_json::from_slice(output)
        .map_err(|error| format!("Windows returned invalid system diagnostics: {error}"))
}

fn host_snapshot() -> Result<HostSnapshot, String> {
    let powershell = powershell().ok_or_else(|| {
        "Windows PowerShell is unavailable; system diagnostics could not be collected".to_string()
    })?;
    let mut command = Command::new(powershell);
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(SNAPSHOT_SCRIPT)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    super::prepare_owned_child(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not collect Windows system diagnostics: {error}"))?;
    super::attach_owned_child(&mut child)
        .map_err(|error| format!("Could not manage Windows diagnostics process: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Windows diagnostics stdout is unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Windows diagnostics stderr is unavailable")?;
    let stdout_reader = thread::spawn(move || {
        super::read_capped_output(BufReader::new(stdout), SNAPSHOT_MAX_OUTPUT_BYTES)
    });
    let stderr_reader = thread::spawn(move || {
        super::read_capped_output(BufReader::new(stderr), SNAPSHOT_MAX_OUTPUT_BYTES)
    });
    let status = super::wait_for_child_with_timeout(
        &mut child,
        SNAPSHOT_TIMEOUT,
        "Windows system diagnostics",
    )?;
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "Windows diagnostics output reader failed")?
        .map_err(|error| format!("Could not read Windows diagnostics output: {error}"))?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| "Windows diagnostics error reader failed")?
        .map_err(|error| format!("Could not read Windows diagnostics errors: {error}"))?;
    if stdout_truncated || stderr_truncated {
        return Err("Windows diagnostics output exceeded the 1 MiB safety limit".into());
    }
    if !status.success() {
        return Err(format!(
            "Windows system diagnostics failed: {}",
            String::from_utf8_lossy(&stderr).trim()
        ));
    }
    snapshot_json(&stdout)
}

pub fn collect() -> Result<Diagnostics, String> {
    let snapshot = host_snapshot()?;
    let gpu = if snapshot.gpu.is_empty() {
        "Unavailable".to_string()
    } else {
        snapshot.gpu.join("; ")
    };
    let dependencies = super::windows_build::dependencies();
    Ok(Diagnostics {
        os: snapshot.os,
        cpu: snapshot.cpu,
        memory_total_bytes: snapshot.memory_total_bytes,
        memory_available_bytes: snapshot.memory_available_bytes,
        gpu,
        controllers: snapshot.controllers,
        session: snapshot.session,
        dependencies,
    })
}

pub fn graphics() -> Result<(Vec<String>, Vec<String>), String> {
    let snapshot = host_snapshot()?;
    Ok((snapshot.graphics_drivers, snapshot.monitors))
}

#[cfg(test)]
mod tests {
    use super::{host_snapshot, snapshot_json};

    #[test]
    fn parses_a_windows_host_snapshot_without_losing_unicode() {
        let snapshot = snapshot_json(
            r#"{"os":"Windows 11 (10.0.26100)","cpu":"Example CPU","memory_total_bytes":17179869184,"memory_available_bytes":8589934592,"gpu":["Écran Graphics"],"graphics_drivers":["Écran Graphics: 1.2.3"],"monitors":["Display"],"controllers":["Xbox Controller"],"session":"Console"}"#
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(snapshot.os, "Windows 11 (10.0.26100)");
        assert_eq!(snapshot.memory_total_bytes, 17_179_869_184);
        assert_eq!(snapshot.gpu, ["Écran Graphics"]);
        assert_eq!(snapshot.graphics_drivers, ["Écran Graphics: 1.2.3"]);
        assert_eq!(snapshot.monitors, ["Display"]);
        assert_eq!(snapshot.controllers, ["Xbox Controller"]);
    }

    #[test]
    fn collects_core_host_diagnostics_on_windows() {
        let snapshot = host_snapshot().expect("Windows PowerShell diagnostics should run");
        assert!(!snapshot.os.is_empty(), "Windows version is reported");
        assert!(!snapshot.cpu.is_empty(), "CPU name has a fallback value");
        assert!(
            snapshot.memory_total_bytes > 0,
            "installed memory is reported"
        );
        assert!(
            snapshot.memory_available_bytes <= snapshot.memory_total_bytes,
            "available memory cannot exceed total memory"
        );
    }

    #[test]
    fn reports_each_windows_build_dependency_once() {
        let dependencies = super::super::windows_build::dependencies();
        let names = dependencies
            .iter()
            .map(|(name, _)| name)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), dependencies.len());
    }
}
