#![forbid(unsafe_code)]

//! XDG application discovery and safe, shell-free launching.

use std::{
    env,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use freedesktop_desktop_entry::DesktopEntry;
use thiserror::Error;

const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationEntry {
    pub desktop_id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub keywords: Vec<String>,
    pub icon: Option<IconReference>,
    pub desktop_path: PathBuf,
    pub exec: Vec<String>,
    pub actions: Vec<String>,
    pub terminal: bool,
    pub dbus_activatable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconReference {
    pub name: String,
    pub resolved_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryDiagnostic {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveryReport {
    pub applications: Vec<ApplicationEntry>,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    pub data_home: PathBuf,
    pub data_dirs: Vec<PathBuf>,
    pub locales: Vec<String>,
    pub current_desktops: Vec<String>,
    pub path: Option<String>,
}

impl DiscoveryOptions {
    pub fn from_environment() -> Self {
        let data_home = env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from(".local/share"));
        let data_dirs = env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| DEFAULT_DATA_DIRS.to_owned())
            .split(':')
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect();
        let locales = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|key| env::var(key).ok())
            .flat_map(|locale| {
                [locale.clone(), locale.split('.').next().unwrap_or(&locale).to_owned()]
            })
            .chain(std::iter::once("C".to_owned()))
            .collect();
        let current_desktops = env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|desktop| !desktop.is_empty())
            .map(str::to_owned)
            .collect();

        Self { data_home, data_dirs, locales, current_desktops, path: env::var("PATH").ok() }
    }
}

pub fn discover() -> DiscoveryReport {
    discover_with_options(&DiscoveryOptions::from_environment())
}

pub fn discover_with_options(options: &DiscoveryOptions) -> DiscoveryReport {
    let mut report = DiscoveryReport::default();
    let mut seen = std::collections::HashSet::new();
    let roots = std::iter::once(&options.data_home).chain(options.data_dirs.iter());

    for root in roots {
        let applications_root = root.join("applications");
        let mut files = Vec::new();
        collect_desktop_files(&applications_root, &mut files, &mut report.diagnostics);
        files.sort();
        for path in files {
            let desktop_id = desktop_id(&applications_root, &path);
            if !seen.insert(desktop_id.clone()) {
                continue;
            }
            match parse_entry(&path, &desktop_id, options) {
                Ok(Some(entry)) => report.applications.push(entry),
                Ok(None) => {}
                Err(reason) => report.diagnostics.push(DiscoveryDiagnostic { path, reason }),
            }
        }
    }

    report.applications.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.desktop_id.cmp(&right.desktop_id))
    });
    report
}

fn collect_desktop_files(
    root: &Path,
    files: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            diagnostics
                .push(DiscoveryDiagnostic { path: root.to_owned(), reason: error.to_string() });
            return;
        }
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                diagnostics.push(DiscoveryDiagnostic { path, reason: error.to_string() });
                continue;
            }
        };
        if file_type.is_dir() {
            collect_desktop_files(&path, files, diagnostics);
        } else if file_type.is_file() && path.extension() == Some(OsStr::new("desktop")) {
            files.push(path);
        }
    }
}

fn desktop_id(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("-")
}

fn parse_entry(
    path: &Path,
    desktop_id: &str,
    options: &DiscoveryOptions,
) -> Result<Option<ApplicationEntry>, String> {
    let entry =
        DesktopEntry::from_path(path, Some(&options.locales)).map_err(|error| error.to_string())?;
    if entry.type_().unwrap_or("Application") != "Application"
        || entry.hidden()
        || entry.no_display()
        || !desktop_visibility_matches(&entry, &options.current_desktops)
    {
        return Ok(None);
    }
    if let Some(try_exec) = entry.try_exec()
        && !executable_available(try_exec, options.path.as_deref())
    {
        return Ok(None);
    }
    let name = entry.name(&options.locales).ok_or("missing localized Name")?.into_owned();
    let exec = parse_exec(entry.exec().ok_or("missing Exec")?, &entry);
    let exec = exec.map_err(|error| error.to_string())?;
    let icon = entry.icon().map(|name| IconReference {
        name: name.to_owned(),
        resolved_path: resolve_icon(name, path, options),
    });
    Ok(Some(ApplicationEntry {
        desktop_id: desktop_id.to_owned(),
        name,
        generic_name: entry.generic_name(&options.locales).map(|value| value.into_owned()),
        keywords: entry
            .keywords(&options.locales)
            .unwrap_or_default()
            .into_iter()
            .map(|value| value.into_owned())
            .collect(),
        icon,
        desktop_path: path.to_owned(),
        exec,
        actions: entry.actions().unwrap_or_default().into_iter().map(str::to_owned).collect(),
        terminal: entry.terminal(),
        dbus_activatable: entry.dbus_activatable(),
    }))
}

fn desktop_visibility_matches(entry: &DesktopEntry, desktops: &[String]) -> bool {
    let only = entry.only_show_in().unwrap_or_default();
    let not = entry.not_show_in().unwrap_or_default();
    (only.is_empty()
        || only
            .iter()
            .any(|desktop| desktops.iter().any(|current| current.eq_ignore_ascii_case(desktop))))
        && !not
            .iter()
            .any(|desktop| desktops.iter().any(|current| current.eq_ignore_ascii_case(desktop)))
}

fn executable_available(value: &str, path: Option<&str>) -> bool {
    let candidate = Path::new(value);
    if candidate.is_absolute() || value.contains('/') {
        return candidate.is_file();
    }
    path.unwrap_or("")
        .split(':')
        .map(Path::new)
        .map(|dir| dir.join(value))
        .any(|candidate| candidate.is_file())
}

fn resolve_icon(name: &str, desktop_path: &Path, options: &DiscoveryOptions) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.is_absolute() && candidate.is_file() {
        return Some(candidate.to_owned());
    }
    let roots = std::iter::once(&options.data_home).chain(options.data_dirs.iter());
    roots
        .flat_map(|root| {
            [
                root.join("icons/hicolor/scalable/apps"),
                root.join("icons/hicolor/48x48/apps"),
                root.join("pixmaps"),
                desktop_path.parent().unwrap_or(Path::new(".")).to_owned(),
            ]
        })
        .flat_map(|dir| {
            [dir.join(name), dir.join(format!("{name}.png")), dir.join(format!("{name}.svg"))]
        })
        .find(|path| path.is_file())
}

fn parse_exec(value: &str, entry: &DesktopEntry) -> Result<Vec<String>, LaunchError> {
    let tokens = shell_like_tokens(value)?;
    if tokens.is_empty() {
        return Err(LaunchError::InvalidExec("Exec field is empty".to_owned()));
    }
    let mut args = Vec::new();
    for token in tokens {
        if let Some(code) = token.strip_prefix('%') {
            if token.len() != 2 {
                return Err(LaunchError::InvalidExec(format!("invalid field code {token}")));
            }
            match code {
                "i" => {
                    if let Some(icon) = entry.icon() {
                        args.push(icon.to_owned());
                    }
                }
                "c" => args.push(entry.name::<String>(&[]).unwrap_or_default().into_owned()),
                "k" => args.push(entry.path.to_string_lossy().into_owned()),
                "f" | "F" | "u" | "U" => {}
                _ => return Err(LaunchError::InvalidExec(format!("unknown field code {token}"))),
            }
        } else {
            args.push(token);
        }
    }
    if args.is_empty() {
        return Err(LaunchError::InvalidExec("Exec field produced no argv".to_owned()));
    }
    Ok(args)
}

fn shell_like_tokens(value: &str) -> Result<Vec<String>, LaunchError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match (quote, character) {
            (Some('"'), '\\') => escaped = true,
            (None, '\\') => escaped = true,
            (Some(expected), character) if expected == character => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, character) if character.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            (_, character) => current.push(character),
        }
    }
    if escaped || quote.is_some() {
        return Err(LaunchError::InvalidExec("unterminated escape or quote".to_owned()));
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

pub trait LaunchBackend {
    fn launch(&mut self, application: &ApplicationEntry) -> Result<(), LaunchError>;
}

#[derive(Debug, Default)]
pub struct ProcessLaunchBackend;

impl LaunchBackend for ProcessLaunchBackend {
    fn launch(&mut self, application: &ApplicationEntry) -> Result<(), LaunchError> {
        if application.terminal {
            return Err(LaunchError::UnsupportedTerminal);
        }
        if application.dbus_activatable {
            return Err(LaunchError::UnsupportedDbusActivation);
        }
        let (program, args) = application
            .exec
            .split_first()
            .ok_or(LaunchError::InvalidExec("Exec field is empty".to_owned()))?;
        let mut command = Command::new(program);
        command.args(args);
        if let Some(parent) = application.desktop_path.parent() {
            command.current_dir(parent);
        }
        command.spawn().map(|_| ()).map_err(LaunchError::Spawn)
    }
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("invalid Exec field: {0}")]
    InvalidExec(String),
    #[error("launching terminal applications is unsupported in Phase 1A")]
    UnsupportedTerminal,
    #[error("D-Bus activation is unsupported in Phase 1A")]
    UnsupportedDbusActivation,
    #[error("failed to spawn application: {0}")]
    Spawn(#[source] io::Error),
    #[error("application exited unsuccessfully: {0}")]
    Exit(ExitStatus),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    fn options(root: &Path) -> DiscoveryOptions {
        DiscoveryOptions {
            data_home: root.join("home"),
            data_dirs: vec![root.join("system")],
            locales: vec!["ru_RU".into(), "C".into()],
            current_desktops: vec!["Niri".into()],
            path: Some("/usr/bin".into()),
        }
    }

    fn write_entry(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn xdg_precedence_and_duplicate_ids_are_deterministic() {
        let temp = tempfile::tempdir().unwrap();
        let opts = options(temp.path());
        write_entry(
            &opts.data_home,
            "applications/org.demo.desktop",
            "[Desktop Entry]\nType=Application\nName=Home\nExec=home\n",
        );
        write_entry(
            &opts.data_dirs[0],
            "applications/org.demo.desktop",
            "[Desktop Entry]\nType=Application\nName=System\nExec=system\n",
        );
        let report = discover_with_options(&opts);
        assert_eq!(report.applications.len(), 1);
        assert_eq!(report.applications[0].name, "Home");
    }

    #[test]
    fn locale_visibility_and_try_exec_are_applied() {
        let temp = tempfile::tempdir().unwrap();
        let opts = options(temp.path());
        write_entry(
            &opts.data_home,
            "applications/demo.desktop",
            "[Desktop Entry]\nType=Application\nName=English\nName[ru_RU]=Русский\nOnlyShowIn=Niri;\nTryExec=definitely-not-installed\nExec=demo\n",
        );
        assert!(discover_with_options(&opts).applications.is_empty());
        let mut visible = opts.clone();
        visible.path = Some("/bin".into());
        write_entry(
            &visible.data_home,
            "applications/visible.desktop",
            "[Desktop Entry]\nType=Application\nName=English\nName[ru_RU]=Русский\nNotShowIn=KDE;\nExec=/bin/echo\n",
        );
        let report = discover_with_options(&visible);
        assert_eq!(report.applications[0].name, "Русский");
    }

    #[test]
    fn malformed_entries_become_diagnostics_and_exec_quotes_are_safe() {
        let temp = tempfile::tempdir().unwrap();
        let opts = options(temp.path());
        write_entry(&opts.data_home, "applications/bad.desktop", "not a group\n");
        write_entry(
            &opts.data_home,
            "applications/good.desktop",
            "[Desktop Entry]\nType=Application\nName=Good\nExec=tool \"hello world\" %c\n",
        );
        let report = discover_with_options(&opts);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.applications[0].exec, vec!["tool", "hello world", "Good"]);
    }

    #[test]
    fn hidden_and_no_display_are_filtered() {
        let temp = tempfile::tempdir().unwrap();
        let opts = options(temp.path());
        write_entry(
            &opts.data_home,
            "applications/hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=Hidden\nHidden=true\nExec=/bin/true\n",
        );
        write_entry(
            &opts.data_home,
            "applications/nodisplay.desktop",
            "[Desktop Entry]\nType=Application\nName=No\nNoDisplay=true\nExec=/bin/true\n",
        );
        assert!(discover_with_options(&opts).applications.is_empty());
        assert_eq!(fs::metadata(&opts.data_home).unwrap().permissions().mode() & 0o777, 0o755);
    }

    struct FakeLaunchBackend {
        launched: Vec<String>,
    }

    impl LaunchBackend for FakeLaunchBackend {
        fn launch(&mut self, application: &ApplicationEntry) -> Result<(), LaunchError> {
            self.launched.push(application.desktop_id.clone());
            Ok(())
        }
    }

    #[test]
    fn launch_backend_is_injectable_without_spawning_a_process() {
        let application = ApplicationEntry {
            desktop_id: "org.demo.desktop".into(),
            name: "Demo".into(),
            generic_name: None,
            keywords: Vec::new(),
            icon: None,
            desktop_path: PathBuf::from("/tmp/demo.desktop"),
            exec: vec!["demo".into()],
            actions: Vec::new(),
            terminal: false,
            dbus_activatable: false,
        };
        let mut backend = FakeLaunchBackend { launched: Vec::new() };
        backend.launch(&application).unwrap();
        assert_eq!(backend.launched, vec!["org.demo.desktop"]);
    }
}
