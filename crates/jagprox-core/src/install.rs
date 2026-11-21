//! How this copy of JagProx was installed, and putting the `jagprox` command on PATH
//! for installs that can't do that themselves (the AppImage, the macOS app).

use std::fs;
use std::path::{Path, PathBuf};

/// How JagProx got onto this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Install {
    /// A plain `jagprox` binary (the Linux tarball or install.sh).
    Standalone {
        exe: PathBuf,
    },
    AppImage {
        path: PathBuf,
    },
    Deb,
    Rpm,
    Aur,
    Nix,
    /// The Windows installer; `dir` is the install folder.
    Windows {
        dir: PathBuf,
    },
    MacApp {
        bundle: PathBuf,
    },
    /// Built from source (`cargo run`, `target/...`).
    Dev,
    Unknown,
}

impl Install {
    pub fn describe(&self) -> &'static str {
        match self {
            Install::Standalone { .. } => "standalone binary",
            Install::AppImage { .. } => "AppImage",
            Install::Deb => ".deb package",
            Install::Rpm => ".rpm package",
            Install::Aur => "AUR package",
            Install::Nix => "Nix package",
            Install::Windows { .. } => "Windows installer",
            Install::MacApp { .. } => "macOS app",
            Install::Dev => "development build",
            Install::Unknown => "unknown install",
        }
    }
}

pub fn current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(fs::canonicalize(&exe).unwrap_or(exe))
}

/// Looks at where this binary lives.
pub fn detect() -> Install {
    let Some(exe) = current_exe() else {
        return Install::Unknown;
    };
    let (appimage, appdir) = appimage_env();
    detect_from(&exe, appimage, appdir, &|path| {
        fs::read_to_string(path).ok()
    })
}

fn appimage_env() -> (Option<PathBuf>, Option<PathBuf>) {
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let appdir = std::env::var_os("APPDIR")
        .map(PathBuf::from)
        .map(|d| fs::canonicalize(&d).unwrap_or(d));
    (appimage, appdir)
}

/// The AppImage file this process was started from, if it was. `APPIMAGE` alone is
/// not proof: it is inherited by everything started from another AppImage app's
/// terminal (Cursor, ...), so this binary must also live inside the mounted `APPDIR`.
pub fn appimage_path() -> Option<PathBuf> {
    let (appimage, appdir) = appimage_env();
    own_appimage(&current_exe()?, appimage, appdir)
}

fn own_appimage(exe: &Path, appimage: Option<PathBuf>, appdir: Option<PathBuf>) -> Option<PathBuf> {
    appimage.filter(|_| appdir.is_some_and(|dir| exe.starts_with(dir)))
}

/// `read` reads a small file (the package marker, Cargo.toml); `None` if missing.
/// `appimage`/`appdir` are the `APPIMAGE`/`APPDIR` variables.
pub fn detect_from(
    exe: &Path,
    appimage: Option<PathBuf>,
    appdir: Option<PathBuf>,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Install {
    let dev = exe.ancestors().any(|dir| {
        dir.file_name().is_some_and(|n| n == "target")
            && dir
                .parent()
                .is_some_and(|p| read(&p.join("Cargo.toml")).is_some())
    });
    if dev {
        return Install::Dev;
    }
    if cfg!(target_os = "linux") {
        if let Some(path) = own_appimage(exe, appimage, appdir) {
            return Install::AppImage { path };
        }
        if exe.starts_with("/nix/store") {
            return Install::Nix;
        }
        if exe.starts_with("/usr") {
            // Packages ship lib/jagprox/package next to bin/ saying what they are.
            let marker = exe
                .parent()
                .and_then(Path::parent)
                .map(|prefix| prefix.join("lib/jagprox/package"))
                .and_then(|path| read(&path));
            match marker.as_deref().map(str::trim) {
                Some("deb") => return Install::Deb,
                Some("rpm") => return Install::Rpm,
                Some("aur") => return Install::Aur,
                _ => {}
            }
        }
        return Install::Standalone {
            exe: exe.to_path_buf(),
        };
    }
    if cfg!(windows) {
        let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        if dir.join("uninstall.exe").is_file() {
            return Install::Windows { dir };
        }
        return Install::Unknown;
    }
    if cfg!(target_os = "macos")
        && let Some(bundle) = exe
            .ancestors()
            .find(|dir| dir.extension().is_some_and(|e| e == "app"))
    {
        return Install::MacApp {
            bundle: bundle.to_path_buf(),
        };
    }
    Install::Unknown
}

/// What the `jagprox` command on PATH should point to for this install.
pub fn cli_target(install: &Install) -> Result<PathBuf, String> {
    match install {
        Install::AppImage { path } => Ok(path.clone()),
        Install::MacApp { bundle } => Ok(bundle.join("Contents/MacOS/jagprox")),
        Install::Standalone { exe } => Ok(exe.clone()),
        Install::Dev => current_exe()
            .and_then(|exe| exe.parent().map(|dir| dir.join(cli_name())))
            .filter(|path| path.is_file())
            .ok_or_else(|| "build the jagprox binary first (cargo build -p jagprox-cli)".into()),
        Install::Deb | Install::Rpm | Install::Aur | Install::Nix => {
            Err("your package already puts jagprox on PATH.".into())
        }
        Install::Windows { .. } => {
            Err("the installer already put jagprox on PATH; open a new terminal to use it.".into())
        }
        Install::Unknown => Err("couldn't tell how JagProx is installed here.".into()),
    }
}

fn cli_name() -> &'static str {
    if cfg!(windows) {
        "jagprox.exe"
    } else {
        "jagprox"
    }
}

/// Where the command link goes: `/usr/local/bin` on macOS (on PATH by default),
/// `~/.local/bin` elsewhere.
pub fn link_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(PathBuf::from("/usr/local/bin/jagprox"))
    } else {
        dirs::home_dir().map(|home| home.join(".local/bin/jagprox"))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Linked {
    /// The link was created or updated.
    Created(PathBuf),
    /// `jagprox` already is that file or a link to it.
    AlreadyThere(PathBuf),
}

/// Creates (or updates) the `jagprox` link at `link` pointing to `target`. Refuses to
/// overwrite a real file that isn't `target`.
#[cfg(unix)]
pub fn link_cli(target: &Path, link: &Path) -> Result<Linked, std::io::Error> {
    use std::io::{Error, ErrorKind};
    if let Ok(existing) = fs::canonicalize(link)
        && fs::canonicalize(target).is_ok_and(|t| t == existing)
    {
        return Ok(Linked::AlreadyThere(link.to_path_buf()));
    }
    match fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(link)?,
        Ok(_) => {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!(
                    "{} already exists and isn't a link; remove it first",
                    link.display()
                ),
            ));
        }
        Err(_) => {}
    }
    if let Some(dir) = link.parent() {
        fs::create_dir_all(dir)?;
    }
    std::os::unix::fs::symlink(target, link)?;
    Ok(Linked::Created(link.to_path_buf()))
}

/// Whether `dir` is one of the folders in PATH.
pub fn on_path(dir: &Path) -> bool {
    let wanted = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths)
            .any(|p| p == wanted || fs::canonicalize(&p).is_ok_and(|p| p == wanted))
    })
}

const PATH_MARKER: &str = "# Added by JagProx (jagprox install-cli)";

/// The shell startup file to add `~/.local/bin` to, and the line to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathFix {
    pub file: PathBuf,
    pub line: String,
}

/// Picks the startup file of the user's shell (`$SHELL`).
pub fn path_fix(home: &Path, shell: Option<&str>, zdotdir: Option<&Path>) -> PathFix {
    let shell = shell
        .and_then(|s| Path::new(s).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let posix = "export PATH=\"$HOME/.local/bin:$PATH\"".to_string();
    match shell.as_str() {
        "fish" => PathFix {
            file: home.join(".config/fish/conf.d/jagprox.fish"),
            line: "fish_add_path --path $HOME/.local/bin".into(),
        },
        "zsh" => PathFix {
            file: zdotdir.unwrap_or(home).join(".zshrc"),
            line: posix,
        },
        "bash" => PathFix {
            file: home.join(".bashrc"),
            line: posix,
        },
        _ => PathFix {
            file: home.join(".profile"),
            line: posix,
        },
    }
}

/// `path_fix` for the current user.
pub fn current_path_fix() -> Option<PathFix> {
    let home = dirs::home_dir()?;
    let shell = std::env::var("SHELL").ok();
    let zdotdir = std::env::var_os("ZDOTDIR").map(PathBuf::from);
    Some(path_fix(&home, shell.as_deref(), zdotdir.as_deref()))
}

/// Appends the PATH line unless it's there already. Returns false if it was.
/// Appends rather than rewrites, so bytes we can't decode (a Latin-1 .zshrc) survive.
pub fn apply_path_fix(fix: &PathFix) -> Result<bool, String> {
    use std::io::Write;
    let failed = |e: std::io::Error| {
        format!(
            "could not edit {} ({e}); add this line yourself: {}",
            fix.file.display(),
            fix.line
        )
    };
    let existing = match fs::read(&fix.file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(failed(e)),
    };
    if String::from_utf8_lossy(&existing).contains(PATH_MARKER) {
        return Ok(false);
    }
    if let Some(dir) = fix.file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut text = String::new();
    if !existing.is_empty() {
        if !existing.ends_with(b"\n") {
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str(&format!("{PATH_MARKER}\n{}\n", fix.line));
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&fix.file)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(failed)?;
    Ok(true)
}

/// macOS: creates the /usr/local/bin link with admin rights. `gui` asks for the
/// password in a system dialog, otherwise sudo asks in the terminal.
#[cfg(target_os = "macos")]
pub fn link_cli_as_admin(target: &Path, link: &Path, gui: bool) -> Result<(), String> {
    use std::process::Command;
    let dir = link.parent().unwrap_or(Path::new("/usr/local/bin"));
    let status = if gui {
        let quote = |p: &Path| {
            let s = p
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            format!("quoted form of \"{s}\"")
        };
        let script = format!(
            "do shell script \"mkdir -p \" & {} & \" && ln -sf \" & {} & \" \" & {} with administrator privileges",
            quote(dir),
            quote(target),
            quote(link)
        );
        Command::new("osascript").arg("-e").arg(script).status()
    } else {
        Command::new("sudo")
            .arg("sh")
            .arg("-c")
            .arg("mkdir -p \"$1\" && ln -sf \"$2\" \"$3\"")
            .arg("sh")
            .arg(dir)
            .arg(target)
            .arg(link)
            .status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err("cancelled".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn same_dir(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_end_matches(['\\', '/']).to_lowercase();
    !a.trim().is_empty() && norm(a) == norm(b)
}

/// Windows `PATH` lists (`;`-separated): adds `dir` at the end. `None` if it's
/// already there (case and a trailing backslash don't matter on Windows).
pub fn windows_path_add(list: &str, dir: &str) -> Option<String> {
    if list.split(';').any(|entry| same_dir(entry, dir)) {
        return None;
    }
    let list = list.trim_end_matches(';');
    Some(if list.is_empty() {
        dir.to_string()
    } else {
        format!("{list};{dir}")
    })
}

/// Removes every `dir` entry. `None` if there was none.
pub fn windows_path_remove(list: &str, dir: &str) -> Option<String> {
    if !list.split(';').any(|entry| same_dir(entry, dir)) {
        return None;
    }
    Some(
        list.split(';')
            .filter(|entry| !same_dir(entry, dir))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::collections::HashMap;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn detect_with(exe: &str, appimage: Option<&str>, files: &[(&str, &str)]) -> Install {
        detect_in(exe, appimage, Some("/tmp/.mount_JagPrxyz"), files)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn detect_in(
        exe: &str,
        appimage: Option<&str>,
        appdir: Option<&str>,
        files: &[(&str, &str)],
    ) -> Install {
        let files: HashMap<PathBuf, String> = files
            .iter()
            .map(|(p, c)| (PathBuf::from(p), c.to_string()))
            .collect();
        detect_from(
            Path::new(exe),
            appimage.map(PathBuf::from),
            appdir.map(PathBuf::from),
            &|p| files.get(p).cloned(),
        )
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_installs() {
        assert_eq!(
            detect_with(
                "/home/me/jagprox/target/release/jagprox",
                None,
                &[("/home/me/jagprox/Cargo.toml", "")]
            ),
            Install::Dev
        );
        assert_eq!(
            detect_with(
                "/tmp/.mount_JagPrxyz/usr/bin/jagprox",
                Some("/home/me/JagProx.AppImage"),
                &[]
            ),
            Install::AppImage {
                path: "/home/me/JagProx.AppImage".into()
            }
        );
        // APPIMAGE inherited from another AppImage app's terminal (Cursor): not ours.
        assert_eq!(
            detect_in(
                "/nix/store/abc-jagprox-3.0.0/bin/jagprox",
                Some("/home/me/Cursor.AppImage"),
                Some("/tmp/.mount_Cursorab"),
                &[]
            ),
            Install::Nix
        );
        assert_eq!(
            detect_in(
                "/home/me/.local/bin/jagprox",
                Some("/home/me/Cursor.AppImage"),
                None,
                &[]
            ),
            Install::Standalone {
                exe: "/home/me/.local/bin/jagprox".into()
            }
        );
        assert_eq!(
            detect_with("/nix/store/abc-jagprox-3.0.0/bin/jagprox", None, &[]),
            Install::Nix
        );
        for (kind, expected) in [
            ("deb\n", Install::Deb),
            ("rpm", Install::Rpm),
            ("aur", Install::Aur),
        ] {
            assert_eq!(
                detect_with(
                    "/usr/bin/jagprox",
                    None,
                    &[("/usr/lib/jagprox/package", kind)]
                ),
                expected
            );
        }
        assert_eq!(
            detect_with("/home/me/.local/bin/jagprox", None, &[]),
            Install::Standalone {
                exe: "/home/me/.local/bin/jagprox".into()
            }
        );
        // Copied into /usr/local/bin by hand: no marker, so it replaces itself.
        assert_eq!(
            detect_with("/usr/local/bin/jagprox", None, &[]),
            Install::Standalone {
                exe: "/usr/local/bin/jagprox".into()
            }
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_installs() {
        assert_eq!(
            detect_with(
                "/Applications/JagProx.app/Contents/MacOS/jagprox",
                None,
                &[]
            ),
            Install::MacApp {
                bundle: "/Applications/JagProx.app".into()
            }
        );
        assert_eq!(
            detect_with(
                "/Users/me/jagprox/target/release/jagprox",
                None,
                &[("/Users/me/jagprox/Cargo.toml", "")]
            ),
            Install::Dev
        );
        assert_eq!(
            detect_with("/usr/local/bin/jagprox", None, &[]),
            Install::Unknown
        );
    }

    #[test]
    fn windows_path_lists() {
        let dir = r"C:\Users\me\AppData\Local\JagProx";
        assert_eq!(windows_path_add("", dir).as_deref(), Some(dir));
        assert_eq!(
            windows_path_add(r"%USERPROFILE%\bin;", dir).unwrap(),
            format!(r"%USERPROFILE%\bin;{dir}")
        );
        assert_eq!(
            windows_path_add(r"C:\x;c:\users\ME\appdata\local\jagprox\", dir),
            None
        );
        assert_eq!(
            windows_path_remove(&format!(r"C:\x;{dir};%USERPROFILE%\bin;{dir}\"), dir).unwrap(),
            r"C:\x;%USERPROFILE%\bin"
        );
        assert_eq!(windows_path_remove(r"C:\x", dir), None);
        // An empty entry never matches.
        assert_eq!(windows_path_remove("C:\\x;;", ""), None);
    }

    #[test]
    fn shell_startup_files() {
        let home = Path::new("/home/me");
        assert_eq!(
            path_fix(home, Some("/usr/bin/zsh"), None).file,
            PathBuf::from("/home/me/.zshrc")
        );
        assert_eq!(
            path_fix(
                home,
                Some("/bin/zsh"),
                Some(Path::new("/home/me/.config/zsh"))
            )
            .file,
            PathBuf::from("/home/me/.config/zsh/.zshrc")
        );
        assert_eq!(
            path_fix(home, Some("/run/current-system/sw/bin/bash"), None).file,
            PathBuf::from("/home/me/.bashrc")
        );
        let fish = path_fix(home, Some("/usr/bin/fish"), None);
        assert_eq!(
            fish.file,
            PathBuf::from("/home/me/.config/fish/conf.d/jagprox.fish")
        );
        assert!(fish.line.starts_with("fish_add_path"));
        assert_eq!(
            path_fix(home, None, None).file,
            PathBuf::from("/home/me/.profile")
        );
    }

    #[test]
    fn path_fix_is_added_once() {
        let dir = std::env::temp_dir().join(format!("jagprox-pathfix-{}", std::process::id()));
        let fix = PathFix {
            file: dir.join(".bashrc"),
            line: "export PATH=\"$HOME/.local/bin:$PATH\"".into(),
        };
        fs::create_dir_all(&dir).unwrap();
        fs::write(&fix.file, "alias ll='ls -l'").unwrap();
        assert!(apply_path_fix(&fix).unwrap());
        assert!(!apply_path_fix(&fix).unwrap());
        let text = fs::read_to_string(&fix.file).unwrap();
        assert_eq!(text.matches(".local/bin").count(), 1);
        assert!(text.starts_with("alias ll='ls -l'\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_fix_keeps_a_non_utf8_file() {
        let dir =
            std::env::temp_dir().join(format!("jagprox-pathfix-latin1-{}", std::process::id()));
        let fix = PathFix {
            file: dir.join(".zshrc"),
            line: "export PATH=\"$HOME/.local/bin:$PATH\"".into(),
        };
        fs::create_dir_all(&dir).unwrap();
        let original = b"# caf\xe9\nalias ll='ls -l'\n";
        fs::write(&fix.file, original).unwrap();
        assert!(apply_path_fix(&fix).unwrap());
        assert!(!apply_path_fix(&fix).unwrap());
        let bytes = fs::read(&fix.file).unwrap();
        assert!(bytes.starts_with(original));
        assert!(bytes.ends_with(format!("{PATH_MARKER}\n{}\n", fix.line).as_bytes()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_created_updated_and_never_clobber_files() {
        let dir = std::env::temp_dir().join(format!("jagprox-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a");
        let b = dir.join("b");
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        let link = dir.join("bin/jagprox");
        assert_eq!(link_cli(&a, &link).unwrap(), Linked::Created(link.clone()));
        assert_eq!(
            link_cli(&a, &link).unwrap(),
            Linked::AlreadyThere(link.clone())
        );
        assert_eq!(link_cli(&b, &link).unwrap(), Linked::Created(link.clone()));
        assert_eq!(fs::read_to_string(&link).unwrap(), "b");
        let file = dir.join("bin/other");
        fs::write(&file, "mine").unwrap();
        assert!(link_cli(&a, &file).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "mine");
        let _ = fs::remove_dir_all(&dir);
    }
}
