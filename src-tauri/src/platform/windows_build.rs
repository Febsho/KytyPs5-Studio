use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    path::PathBuf,
    process::{Command, Stdio},
};

const BUILD_ENVIRONMENT: &[&str] = &[
    "PATH",
    "INCLUDE",
    "LIB",
    "LIBPATH",
    "WindowsSdkDir",
    "WindowsSDKVersion",
    "WindowsSDKLibVersion",
    "UniversalCRTSdkDir",
    "UCRTVersion",
    "VCToolsInstallDir",
    "VCINSTALLDIR",
    "VSINSTALLDIR",
    "VisualStudioVersion",
    "DevEnvDir",
    "VSCMD_VER",
    "VSCMD_ARG_TGT_ARCH",
    "VSCMD_ARG_HOST_ARCH",
    "WindowsLibPath",
];

fn vswhere_path() -> Result<PathBuf, String> {
    let program_files = env::var_os("ProgramFiles(x86)")
        .ok_or("ProgramFiles(x86) is unavailable; install Visual Studio 2022 Build Tools")?;
    let path = PathBuf::from(program_files)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    path.is_file().then_some(path).ok_or_else(|| {
        "Visual Studio Installer (vswhere.exe) is missing; install Visual Studio 2022 Build Tools with the Desktop development with C++ workload".into()
    })
}

fn visual_studio_installation() -> Result<PathBuf, String> {
    let output = Command::new(vswhere_path()?)
        .args([
            "-latest",
            "-version",
            "[17.0,18.0)",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .map_err(|error| format!("Could not query Visual Studio Build Tools: {error}"))?;
    if !output.status.success() {
        return Err(
            "Visual Studio Build Tools discovery failed; repair the C++ workload and retry".into(),
        );
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err("Visual Studio C++ Build Tools were not found. Install the Desktop development with C++ workload and Windows 10/11 SDK.".into());
    }
    let path = PathBuf::from(path);
    let dev_command = path.join("Common7").join("Tools").join("VsDevCmd.bat");
    if !dev_command.is_file() {
        return Err(format!(
            "Visual Studio developer environment is missing at {}",
            dev_command.display()
        ));
    }
    Ok(path)
}

fn path_has_executable(name: &str) -> bool {
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path).any(|directory| directory.join(name).is_file())
}

fn version_output_matches(stdout: &[u8], stderr: &[u8], expected: &str) -> bool {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout.lines().chain(stderr.lines()).any(|line| {
        line.trim_start()
            .get(..expected.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected))
    })
}

fn command_reports(program: &str, args: &[&str], expected: &str) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .is_some_and(|output| {
            output.status.success()
                && version_output_matches(&output.stdout, &output.stderr, expected)
        })
}

fn command_succeeds(program: &std::ffi::OsStr, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .is_some_and(|output| {
            output.status.success() && (!output.stdout.is_empty() || !output.stderr.is_empty())
        })
}

fn python3_available() -> bool {
    ["python.exe", "python3.exe"]
        .into_iter()
        .any(|program| command_reports(program, &["--version"], "Python 3."))
}

fn glslang_available() -> bool {
    ["glslangValidator.exe", "glslang.exe"]
        .into_iter()
        .any(|program| command_succeeds(std::ffi::OsStr::new(program), &["--version"]))
        || env::var_os("VULKAN_SDK")
            .map(PathBuf::from)
            .is_some_and(|sdk| glslang_in_directory(&sdk.join("Bin")))
}

fn glslang_in_directory(directory: &std::path::Path) -> bool {
    ["glslangValidator.exe", "glslang.exe"]
        .into_iter()
        .any(|name| directory.join(name).is_file())
}

fn windows_sdk_available() -> bool {
    let mut roots = Vec::new();
    if let Some(root) = env::var_os("WindowsSdkDir") {
        roots.push(PathBuf::from(root));
    }
    if let Some(program_files) = env::var_os("ProgramFiles(x86)") {
        roots.push(PathBuf::from(program_files).join("Windows Kits").join("10"));
    }
    roots
        .into_iter()
        .any(|root| windows_sdk_root_available(&root))
}

fn windows_sdk_root_available(root: &std::path::Path) -> bool {
    std::fs::read_dir(root.join("Include"))
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|version| {
            let version = version.file_name();
            let include = root.join("Include").join(&version);
            let libraries = root.join("Lib").join(version);
            include.join("ucrt/stdio.h").is_file()
                && include.join("um/Windows.h").is_file()
                && libraries.join("ucrt/x64/ucrt.lib").is_file()
                && libraries.join("um/x64/kernel32.lib").is_file()
        })
}

fn cmake_version_at_least_3_22_1(output: &str) -> bool {
    let Some(version) = output
        .lines()
        .find_map(|line| line.strip_prefix("cmake version "))
    else {
        return false;
    };
    let version = version.split_whitespace().next().unwrap_or_default();
    let parts = version.split('.').take(3).collect::<Vec<_>>();
    if parts.len() != 3 {
        return false;
    }
    let mut parsed = Vec::with_capacity(3);
    for part in &parts {
        let digits = part
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        let Ok(number) = digits.parse::<u64>() else {
            return false;
        };
        parsed.push(number);
    }
    match parsed.as_slice().cmp(&[3, 22, 1]) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            // A prerelease of the minimum version is still below the required
            // stable release. Build metadata does not affect version ordering.
            !parts[2].contains('-')
        }
    }
}

fn append_vulkan_sdk_environment(
    environment: &mut Vec<(OsString, OsString)>,
    sdk: Option<OsString>,
) -> Result<(), String> {
    let Some(sdk) = sdk else {
        return Ok(());
    };
    let sdk_bin = PathBuf::from(&sdk).join("Bin");
    if glslang_in_directory(&sdk_bin) {
        if let Some((_, value)) = environment
            .iter_mut()
            .find(|(name, _)| name.to_string_lossy().eq_ignore_ascii_case("PATH"))
        {
            let mut paths = vec![sdk_bin];
            paths.extend(env::split_paths(value));
            *value = env::join_paths(paths)
                .map_err(|error| format!("Could not add Vulkan SDK tools to PATH: {error}"))?;
        }
    }
    // CMake's FindVulkan module uses VULKAN_SDK to locate headers and
    // libraries. Adding only Bin to PATH finds glslangValidator but is not
    // enough to configure an emulator build.
    environment.push((OsString::from("VULKAN_SDK"), sdk));
    Ok(())
}

fn clang_directory(
    installation: &std::path::Path,
    fallback_path: &std::ffi::OsStr,
) -> Option<PathBuf> {
    let bundled = installation.join("VC/Tools/Llvm/bin");
    if bundled.join("clang-cl.exe").is_file() {
        return Some(bundled);
    }
    env::split_paths(fallback_path).find(|directory| directory.join("clang-cl.exe").is_file())
}

fn append_clang_environment(
    environment: &mut [(OsString, OsString)],
    installation: &std::path::Path,
    fallback_path: &std::ffi::OsStr,
) -> Result<(), String> {
    let Some(clang_bin) = clang_directory(installation, fallback_path) else {
        return Ok(());
    };
    if let Some((_, path)) = environment
        .iter_mut()
        .find(|(name, _)| name.to_string_lossy().eq_ignore_ascii_case("PATH"))
    {
        let mut paths = vec![clang_bin];
        paths.extend(env::split_paths(path));
        *path = env::join_paths(paths)
            .map_err(|error| format!("Could not add clang-cl to PATH: {error}"))?;
    }
    Ok(())
}

fn cmake_available() -> bool {
    path_has_executable("cmake.exe")
        && Command::new("cmake.exe")
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| cmake_version_at_least_3_22_1(&String::from_utf8_lossy(&output.stdout)))
            .unwrap_or(false)
}

pub fn dependencies() -> Vec<(String, bool)> {
    let visual_studio = visual_studio_installation();
    let llvm = visual_studio
        .as_ref()
        .is_ok_and(|path| path.join("VC/Tools/Llvm/bin/clang-cl.exe").is_file())
        || path_has_executable("clang-cl.exe");
    vec![
        (
            "Git for Windows".into(),
            command_reports("git.exe", &["--version"], "git version "),
        ),
        ("CMake 3.22.1+".into(), cmake_available()),
        (
            "Ninja".into(),
            command_reports("ninja.exe", &["--version"], ""),
        ),
        (
            "Visual Studio C++ Build Tools".into(),
            visual_studio.is_ok(),
        ),
        ("Windows 10/11 SDK".into(), windows_sdk_available()),
        ("clang-cl".into(), llvm),
        ("glslangValidator".into(), glslang_available()),
        ("Python 3".into(), python3_available()),
    ]
}

pub fn validate() -> Result<(), String> {
    let missing = dependencies()
        .into_iter()
        .filter_map(|(name, present)| (!present).then_some(name))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing_dependency_error(&missing))
    }
}

fn missing_dependency_error(missing: &[String]) -> String {
    let mut guidance = Vec::new();
    if missing.iter().any(|name| name == "Git for Windows") {
        guidance.push("Install Git for Windows and add it to PATH.");
    }
    if missing.iter().any(|name| {
        matches!(
            name.as_str(),
            "Visual Studio C++ Build Tools" | "Windows 10/11 SDK"
        )
    }) {
        guidance.push(
            "Install Visual Studio 2022 Build Tools with the Desktop development with C++ workload and Windows 10/11 SDK.",
        );
    }
    if missing.iter().any(|name| name == "clang-cl") {
        guidance.push(
            "Install the C++ Clang tools component in Visual Studio, or add clang-cl.exe to PATH.",
        );
    }
    if missing.iter().any(|name| {
        matches!(
            name.as_str(),
            "CMake 3.22.1+" | "Ninja" | "glslangValidator" | "Python 3"
        )
    }) {
        guidance.push(
            "Install the missing build tools and add them to PATH; glslang or glslangValidator is also provided by the Vulkan SDK.",
        );
    }
    format!(
        "Missing Windows emulator build requirements: {}. {}",
        missing.join(", "),
        guidance.join(" ")
    )
}

pub fn cmake_arguments(build_type: &str) -> Result<Vec<String>, String> {
    validate()?;
    Ok(configuration_arguments(build_type))
}

fn configuration_arguments(build_type: &str) -> Vec<String> {
    vec![
        "-G".into(),
        "Ninja".into(),
        format!("-DCMAKE_BUILD_TYPE={build_type}"),
        "-DCMAKE_C_COMPILER=clang-cl".into(),
        "-DCMAKE_CXX_COMPILER=clang-cl".into(),
    ]
}

pub fn display_cmake_arguments(build_type: &str) -> Result<Vec<String>, String> {
    let arguments = cmake_arguments(build_type)?;
    let mut display = Vec::new();
    let mut iter = arguments.into_iter();
    while let Some(argument) = iter.next() {
        if argument == "-G" {
            if let Some(generator) = iter.next() {
                display.push(format!("-G {generator}"));
            }
        } else {
            display.push(argument);
        }
    }
    Ok(display)
}

fn command_environment() -> Result<Vec<(OsString, OsString)>, String> {
    let installation = visual_studio_installation()?;
    let developer_command = installation
        .join("Common7")
        .join("Tools")
        .join("VsDevCmd.bat");
    let command_line = format!(
        "call \"{}\" -no_logo -arch=x64 -host_arch=x64 && ({} )",
        developer_command.display(),
        BUILD_ENVIRONMENT
            .iter()
            .map(|name| format!("set {name}"))
            .chain(std::iter::once("ver >nul".to_string()))
            .collect::<Vec<_>>()
            .join("&")
    );
    let mut command = Command::new("cmd.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // cmd.exe's /c parser does not follow the normal Windows argv quoting
        // rules, so pass its complete command line verbatim.
        command.raw_arg(format!("/d /u /c \"{command_line}\""));
    }
    #[cfg(not(windows))]
    command.args(["/d", "/u", "/c", &command_line]);
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            format!("Could not initialize the Visual Studio x64 environment: {error}")
        })?;
    if !output.status.success() {
        let stdout = decode_command_output(&output.stdout)
            .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).into_owned());
        let stderr = decode_command_output(&output.stderr)
            .unwrap_or_else(|_| String::from_utf8_lossy(&output.stderr).into_owned());
        return Err(format!(
            "Visual Studio x64 environment setup failed with {}. stdout: {} stderr: {}. Verify the VS 2022 x64 C++ workload and Windows SDK.",
            output.status,
            stdout.trim(),
            stderr.trim()
        ));
    }
    let allowed = BUILD_ENVIRONMENT
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let decoded_stdout = decode_command_output(&output.stdout)
        .map_err(|error| format!("Could not decode Visual Studio environment output: {error}"))?;
    let mut environment = decoded_stdout
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            allowed
                .contains(&name.to_ascii_lowercase())
                .then(|| (OsString::from(name), OsString::from(value)))
        })
        .collect::<Vec<_>>();
    append_clang_environment(
        &mut environment,
        &installation,
        &env::var_os("PATH").unwrap_or_default(),
    )?;
    append_vulkan_sdk_environment(&mut environment, env::var_os("VULKAN_SDK"))?;
    for required in ["PATH", "INCLUDE", "LIB", "WindowsSdkDir"] {
        if !environment.iter().any(|(name, value)| {
            name.to_string_lossy().eq_ignore_ascii_case(required) && !value.is_empty()
        }) {
            return Err(format!(
                "Visual Studio environment is missing {required}; install the x64 MSVC tools and Windows SDK"
            ));
        }
    }
    Ok(environment)
}

fn decode_command_output(bytes: &[u8]) -> Result<String, String> {
    let bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
    if !bytes.len().is_multiple_of(2) {
        return Err("Windows command output has an incomplete UTF-16 code unit".into());
    }
    let units = bytes
        .chunks(2)
        .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units)
        .map_err(|error| format!("Windows command output is invalid UTF-16: {error}"))
}

pub fn apply_environment(command: &mut Command) -> Result<(), String> {
    for (name, value) in command_environment()? {
        command.env(name, value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        cmake_version_at_least_3_22_1, configuration_arguments, missing_dependency_error,
        windows_sdk_root_available,
    };
    use std::{
        ffi::OsString,
        path::{Path, PathBuf},
    };

    #[test]
    fn enforces_the_upstream_windows_cmake_minimum() {
        assert!(cmake_version_at_least_3_22_1("cmake version 3.22.1"));
        assert!(cmake_version_at_least_3_22_1("cmake version 4.0.0-rc1"));
        assert!(cmake_version_at_least_3_22_1(
            "cmake version 3.22.1+build.7"
        ));
        assert!(!cmake_version_at_least_3_22_1("cmake version 3.22.0"));
        assert!(!cmake_version_at_least_3_22_1("cmake version 3.22.1-rc1"));
        assert!(!cmake_version_at_least_3_22_1("CMake version unknown"));
    }

    #[test]
    fn dependency_probe_accepts_version_output_on_stderr_but_requires_python_three() {
        assert!(super::version_output_matches(
            b"",
            b"Python 3.12.4\r\n",
            "Python 3."
        ));
        assert!(!super::version_output_matches(
            b"Python 2.7.18\n",
            b"",
            "Python 3."
        ));
        assert!(super::version_output_matches(
            b"git version 2.50.1.windows.1\n",
            b"",
            "git version "
        ));
        assert!(!super::version_output_matches(b"", b"", ""));
    }

    #[test]
    fn decodes_visual_studio_environment_output_as_unicode() {
        let text = "PATH=C:\\Users\\Jörg\\Tools\r\nVULKAN_SDK=C:\\Vulkan SDK\\遊戲\r\n";
        let bytes = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(super::decode_command_output(&bytes).unwrap(), text);
        assert!(super::decode_command_output(&[0x41]).is_err());
    }

    #[test]
    fn preserves_the_vulkan_sdk_root_for_cmake_and_adds_its_tools_to_path() {
        let sdk = std::env::temp_dir().join(format!(
            "kyty-vulkan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bin = sdk.join("Bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("glslangValidator.exe"), b"test").unwrap();
        let original_path =
            std::env::join_paths([std::path::PathBuf::from("existing tools")]).unwrap();
        let mut environment = vec![(std::ffi::OsString::from("PATH"), original_path)];

        super::append_vulkan_sdk_environment(&mut environment, Some(sdk.clone().into())).unwrap();

        let sdk_entry = environment
            .iter()
            .find(|(name, _)| name == "VULKAN_SDK")
            .map(|(_, value)| std::path::PathBuf::from(value));
        assert_eq!(sdk_entry.as_deref(), Some(sdk.as_path()));
        let path = environment
            .iter()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| std::env::split_paths(value).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(path.first(), Some(&bin));
        assert_eq!(
            path.get(1),
            Some(&std::path::PathBuf::from("existing tools"))
        );
        std::fs::remove_dir_all(sdk).unwrap();
    }

    #[test]
    fn accepts_either_upstream_glslang_executable_name() {
        let root = std::env::temp_dir().join(format!(
            "kyty-glslang-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(!super::glslang_in_directory(&root));
        std::fs::write(root.join("glslang.exe"), b"fixture").unwrap();
        assert!(super::glslang_in_directory(&root));
        std::fs::remove_file(root.join("glslang.exe")).unwrap();
        std::fs::write(root.join("glslangValidator.exe"), b"fixture").unwrap();
        assert!(super::glslang_in_directory(&root));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn standalone_llvm_accepted_by_preflight_is_added_to_the_build_path() {
        let root = std::env::temp_dir().join(format!(
            "kyty-clang-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let installation = root.join("Visual Studio");
        let standalone = root.join("LLVM Tools");
        std::fs::create_dir_all(&standalone).unwrap();
        std::fs::write(standalone.join("clang-cl.exe"), b"fixture").unwrap();
        let fallback_path = std::env::join_paths([standalone.clone()]).unwrap();
        let original_path = std::env::join_paths([PathBuf::from("Visual Studio tools")]).unwrap();
        let mut environment = vec![(OsString::from("PATH"), original_path)];

        super::append_clang_environment(&mut environment, &installation, &fallback_path).unwrap();

        let path = environment
            .iter()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| std::env::split_paths(value).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(path.first(), Some(&standalone));
        assert_eq!(path.get(1), Some(&PathBuf::from("Visual Studio tools")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configures_native_x64_builds_with_ninja_and_clang_cl() {
        assert_eq!(
            configuration_arguments("Release"),
            [
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Release",
                "-DCMAKE_C_COMPILER=clang-cl",
                "-DCMAKE_CXX_COMPILER=clang-cl",
            ]
        );
    }

    #[test]
    fn missing_git_has_an_actionable_windows_install_message() {
        let error = missing_dependency_error(&["Git for Windows".into()]);
        assert!(error.contains("Git for Windows"));
        assert!(error.contains("add it to PATH"));
        assert!(!error.contains("Visual Studio"));
    }

    #[test]
    fn windows_sdk_detection_requires_the_build_headers_and_x64_libraries() {
        let root = std::env::temp_dir().join(format!(
            "kyty-windows-sdk-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let include = root.join("Include/10.0.26100.0");
        let libraries = root.join("Lib/10.0.26100.0");
        for file in [
            include.join("ucrt/stdio.h"),
            include.join("um/Windows.h"),
            libraries.join("ucrt/x64/ucrt.lib"),
        ] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"fixture").unwrap();
        }
        assert!(!windows_sdk_root_available(Path::new(&root)));

        let kernel32 = libraries.join("um/x64/kernel32.lib");
        std::fs::create_dir_all(kernel32.parent().unwrap()).unwrap();
        std::fs::write(kernel32, b"fixture").unwrap();
        assert!(windows_sdk_root_available(Path::new(&root)));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_toolchain_components_have_specific_install_guidance() {
        let error = missing_dependency_error(&[
            "Visual Studio C++ Build Tools".into(),
            "clang-cl".into(),
            "glslangValidator".into(),
        ]);
        assert!(error.contains("Desktop development with C++"));
        assert!(error.contains("C++ Clang tools component"));
        assert!(error.contains("Vulkan SDK"));
        assert!(!error.contains("Git for Windows"));
    }

    #[cfg(windows)]
    #[test]
    fn captures_the_installed_visual_studio_x64_environment() {
        let environment = match super::command_environment() {
            Ok(environment) => environment,
            Err(error)
                if std::env::var("KYTY_REQUIRE_VISUAL_STUDIO_ENV")
                    .ok()
                    .as_deref()
                    != Some("true") =>
            {
                assert!(error.contains("Visual Studio"), "{error}");
                return;
            }
            Err(error) => {
                panic!("Windows CI provides a working Visual Studio environment: {error}")
            }
        };
        for required in ["PATH", "INCLUDE", "LIB", "WindowsSdkDir"] {
            assert!(
                environment.iter().any(|(name, value)| {
                    name.to_string_lossy().eq_ignore_ascii_case(required) && !value.is_empty()
                }),
                "captured Visual Studio environment should include {required}"
            );
        }
        assert!(
            environment.iter().any(|(name, value)| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case("VSCMD_ARG_TGT_ARCH")
                    && value.to_string_lossy().eq_ignore_ascii_case("x64")
            }),
            "captured Visual Studio environment targets x64"
        );
        if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
            assert!(
                environment.iter().any(|(name, value)| {
                    name.to_string_lossy().eq_ignore_ascii_case("VULKAN_SDK") && *value == sdk
                }),
                "captured build environment preserves the Vulkan SDK root"
            );
        }
        for (program, args) in [
            ("cmake.exe", vec!["--version"]),
            ("ninja.exe", vec!["--version"]),
            ("clang-cl.exe", vec!["--version"]),
        ] {
            let output = std::process::Command::new(program)
                .envs(environment.iter().cloned())
                .args(args)
                .output()
                .unwrap_or_else(|error| {
                    panic!("captured Visual Studio environment launches {program}: {error}")
                });
            assert!(
                output.status.success(),
                "{program} runs with the captured Visual Studio environment: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
