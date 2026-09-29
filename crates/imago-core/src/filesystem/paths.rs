//! Output paths: destination folder, folder structure, suffix, extension,
//! path-traversal protection and name-conflict resolution.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use crate::errors::{ImagoError, Result};
use crate::filesystem::scan::InputFile;
use crate::settings::{ConflictPolicy, ImageFormat, OutputLocation, OutputSettings};

/// Computes the output path for `input` written as `format` (before conflict
/// resolution).
pub fn output_path(
    input: &InputFile,
    format: ImageFormat,
    settings: &OutputSettings,
) -> Result<PathBuf> {
    if let OutputLocation::File { path } = &settings.location {
        return Ok(exact_file_path(path, format));
    }
    let destination = destination_dir(input, settings);
    let relative_dir = if settings.preserve_structure {
        relative_parent(input)?
    } else {
        PathBuf::new()
    };
    let file_name = output_file_name(&input.path, format, &settings.suffix)?;
    let path = destination.join(&relative_dir).join(file_name);
    ensure_within(&destination, &path)?;
    Ok(path)
}

/// The folder all outputs of `input` go to (without the relative structure).
pub fn destination_dir(input: &InputFile, settings: &OutputSettings) -> PathBuf {
    match &settings.location {
        OutputLocation::Subfolder { name } => input.base_dir().join(name.trim()),
        OutputLocation::Directory { path } => path.clone(),
        OutputLocation::SameFolder => input.base_dir().to_path_buf(),
        OutputLocation::File { path } => path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    }
}

/// The user's exact output path, with the extension corrected to `format`
/// when it does not already name that format (ADR-008).
pub fn exact_file_path(path: &Path, format: ImageFormat) -> PathBuf {
    let fits = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            format
                .extensions()
                .contains(&extension.to_ascii_lowercase().as_str())
        });
    if fits {
        path.to_path_buf()
    } else {
        path.with_extension(format.extension())
    }
}

/// Folder of `input` relative to its root, made only of normal components.
fn relative_parent(input: &InputFile) -> Result<PathBuf> {
    let (Some(root), Some(parent)) = (input.root.as_deref(), input.path.parent()) else {
        return Ok(PathBuf::new());
    };
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| ImagoError::PathTraversal {
            path: input.path.clone(),
        })?;
    sanitize_relative(relative).ok_or_else(|| ImagoError::PathTraversal {
        path: input.path.clone(),
    })
}

/// Keeps a relative path only if every component is a plain name.
pub fn sanitize_relative(path: &Path) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(clean)
}

/// `stem + suffix + "." + extension`. Keeps the input's own extension spelling
/// when it already fits the output format (`photo.jpeg` stays `.jpeg`).
pub fn output_file_name(input: &Path, format: ImageFormat, suffix: &str) -> Result<OsString> {
    let stem = input
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| ImagoError::PathTraversal {
            path: input.to_path_buf(),
        })?;
    let extension = input
        .extension()
        .and_then(OsStr::to_str)
        .filter(|extension| {
            format
                .extensions()
                .contains(&extension.to_ascii_lowercase().as_str())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| format.extension().to_owned());
    let mut name = OsString::from(stem);
    name.push(suffix);
    name.push(".");
    name.push(extension);
    let candidate = Path::new(&name);
    // The name must be a single, normal component (no separators from the suffix).
    if candidate.components().count() != 1
        || !matches!(candidate.components().next(), Some(Component::Normal(_)))
    {
        return Err(ImagoError::PathTraversal {
            path: candidate.to_path_buf(),
        });
    }
    Ok(name)
}

/// Rejects `path` unless it lies inside `base` (lexically).
pub fn ensure_within(base: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(base)
        .map_err(|_| ImagoError::PathTraversal {
            path: path.to_path_buf(),
        })?;
    if sanitize_relative(relative).is_none() {
        return Err(ImagoError::PathTraversal {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// `photo.webp` → `photo (1).webp`.
pub fn numbered(path: &Path, number: u32) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default();
    let mut name = OsString::from(stem);
    name.push(format!(" ({number})"));
    if let Some(extension) = path.extension() {
        name.push(".");
        name.push(extension);
    }
    path.with_file_name(name)
}

/// Output paths already taken by files of the running job, so two inputs that
/// map to the same name (`a.png` and `a.jpg` → `a.webp`) never overwrite each other.
#[derive(Debug, Default)]
pub struct ClaimedPaths {
    claimed: Mutex<HashSet<PathBuf>>,
}

/// What to do with a computed output path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Write to this path (it does not exist yet).
    Create(PathBuf),
    /// Replace the existing file at this path.
    Replace(PathBuf),
    /// Leave the existing file and skip the input.
    Skip(PathBuf),
}

impl Resolution {
    /// The path involved.
    pub fn path(&self) -> &Path {
        match self {
            Resolution::Create(path) | Resolution::Replace(path) | Resolution::Skip(path) => path,
        }
    }
}

/// Maximum number of `(n)` suffixes tried before giving up.
const MAX_RENAME_ATTEMPTS: u32 = 10_000;

impl ClaimedPaths {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves conflicts for `wanted` and claims the result for this job.
    ///
    /// * never returns the original input unless `allow_overwrite_originals`;
    /// * paths claimed earlier in the same job are always avoided by renaming.
    pub fn resolve(
        &self,
        input: &Path,
        wanted: PathBuf,
        settings: &OutputSettings,
    ) -> Result<Resolution> {
        let mut claimed = self
            .claimed
            .lock()
            .map_err(|_| ImagoError::Internal("output path registry is poisoned".into()))?;
        let is_original = |path: &Path| same_file::is_same_file(input, path).unwrap_or(false);

        let exists = wanted.exists();
        if !claimed.contains(&wanted) {
            if !exists {
                claimed.insert(wanted.clone());
                return Ok(Resolution::Create(wanted));
            }
            match settings.conflict {
                ConflictPolicy::Skip => return Ok(Resolution::Skip(wanted)),
                ConflictPolicy::Overwrite => {
                    if is_original(&wanted) && !settings.allow_overwrite_originals {
                        return Err(ImagoError::WouldOverwriteOriginal { path: wanted });
                    }
                    claimed.insert(wanted.clone());
                    return Ok(Resolution::Replace(wanted));
                }
                ConflictPolicy::Rename => {}
            }
        }
        for number in 1..=MAX_RENAME_ATTEMPTS {
            let candidate = numbered(&wanted, number);
            if !claimed.contains(&candidate) && !candidate.exists() {
                claimed.insert(candidate.clone());
                return Ok(Resolution::Create(candidate));
            }
        }
        Err(ImagoError::OutputExists { path: wanted })
    }

    /// Frees a claimed path (e.g. when the file was skipped or failed).
    pub fn release(&self, path: &Path) {
        if let Ok(mut claimed) = self.claimed.lock() {
            claimed.remove(path);
        }
    }

    /// Replaces a claim after the writer had to pick another name.
    pub fn claim(&self, path: &Path) -> bool {
        self.claimed
            .lock()
            .map(|mut claimed| claimed.insert(path.to_path_buf()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DEFAULT_SUBFOLDER;

    fn input(path: &str, root: Option<&str>) -> InputFile {
        InputFile {
            path: PathBuf::from(path),
            root: root.map(PathBuf::from),
            size: 1,
        }
    }

    #[test]
    fn default_output_goes_to_optimized_subfolder() {
        let settings = OutputSettings::default();
        let path =
            output_path(&input("/photos/a.png", None), ImageFormat::Webp, &settings).unwrap();
        assert_eq!(
            path,
            Path::new("/photos").join(DEFAULT_SUBFOLDER).join("a.webp")
        );
    }

    #[test]
    fn folder_structure_is_preserved_below_the_root() {
        let settings = OutputSettings {
            location: OutputLocation::Directory {
                path: PathBuf::from("/out"),
            },
            ..OutputSettings::default()
        };
        let file = input("/photos/2024/june/b.jpg", Some("/photos"));
        let path = output_path(&file, ImageFormat::Avif, &settings).unwrap();
        assert_eq!(
            path,
            Path::new("/out").join("2024").join("june").join("b.avif")
        );

        let flat = OutputSettings {
            preserve_structure: false,
            ..settings
        };
        assert_eq!(
            output_path(&file, ImageFormat::Avif, &flat).unwrap(),
            Path::new("/out").join("b.avif")
        );
    }

    #[test]
    fn suffix_and_extension_spelling() {
        let settings = OutputSettings {
            location: OutputLocation::SameFolder,
            suffix: "-min".into(),
            ..OutputSettings::default()
        };
        let path =
            output_path(&input("/p/photo.JPEG", None), ImageFormat::Jpeg, &settings).unwrap();
        assert_eq!(path, Path::new("/p").join("photo-min.JPEG"));
        let path =
            output_path(&input("/p/photo.jpeg", None), ImageFormat::Webp, &settings).unwrap();
        assert_eq!(path, Path::new("/p").join("photo-min.webp"));
    }

    #[test]
    fn unicode_names_are_kept() {
        let settings = OutputSettings::default();
        let path = output_path(
            &input("/zdjęcia/żółw 🐢.png", None),
            ImageFormat::Png,
            &settings,
        )
        .unwrap();
        assert!(path.ends_with(Path::new("optimized").join("żółw 🐢.png")));
    }

    #[test]
    fn traversal_is_rejected() {
        assert!(sanitize_relative(Path::new("a/../b")).is_none());
        assert!(sanitize_relative(Path::new("/abs")).is_none());
        assert_eq!(
            sanitize_relative(Path::new("a/./b")).unwrap(),
            Path::new("a").join("b")
        );
        assert!(ensure_within(Path::new("/out"), Path::new("/out/../etc/passwd")).is_err());
        assert!(ensure_within(Path::new("/out"), Path::new("/elsewhere/x.png")).is_err());
        assert!(output_file_name(Path::new("a.png"), ImageFormat::Png, "/../../x").is_err());
        // A file outside its declared root cannot produce an output path.
        let file = input("/other/c.png", Some("/photos"));
        assert!(output_path(&file, ImageFormat::Png, &OutputSettings::default()).is_err());
    }

    #[test]
    fn exact_file_output_corrects_the_extension() {
        let settings = OutputSettings {
            location: OutputLocation::File {
                path: PathBuf::from("/o/optimized.jpg"),
            },
            ..OutputSettings::default()
        };
        let file = input("/in/input.png", None);
        assert_eq!(
            output_path(&file, ImageFormat::Jpeg, &settings).unwrap(),
            Path::new("/o/optimized.jpg")
        );
        assert_eq!(
            output_path(&file, ImageFormat::Webp, &settings).unwrap(),
            Path::new("/o/optimized.webp")
        );
        assert_eq!(destination_dir(&file, &settings), Path::new("/o"));
    }

    #[test]
    fn numbering_keeps_the_extension() {
        assert_eq!(
            numbered(Path::new("/o/a.webp"), 2),
            Path::new("/o/a (2).webp")
        );
        assert_eq!(
            numbered(Path::new("/o/noext"), 1),
            Path::new("/o/noext (1)")
        );
    }

    #[test]
    fn conflicts_are_resolved_per_policy() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("photo.png");
        std::fs::write(&original, b"original").unwrap();
        let existing = dir.path().join("photo.webp");
        std::fs::write(&existing, b"old").unwrap();

        let rename = OutputSettings::default();
        let claims = ClaimedPaths::new();
        let resolution = claims
            .resolve(&original, existing.clone(), &rename)
            .unwrap();
        assert_eq!(
            resolution,
            Resolution::Create(dir.path().join("photo (1).webp"))
        );

        let skip = OutputSettings {
            conflict: ConflictPolicy::Skip,
            ..OutputSettings::default()
        };
        assert_eq!(
            ClaimedPaths::new()
                .resolve(&original, existing.clone(), &skip)
                .unwrap(),
            Resolution::Skip(existing.clone())
        );

        let overwrite = OutputSettings {
            conflict: ConflictPolicy::Overwrite,
            ..OutputSettings::default()
        };
        assert_eq!(
            ClaimedPaths::new()
                .resolve(&original, existing.clone(), &overwrite)
                .unwrap(),
            Resolution::Replace(existing)
        );
    }

    #[test]
    fn original_is_protected_without_permission() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("photo.png");
        std::fs::write(&original, b"original").unwrap();
        let overwrite = OutputSettings {
            conflict: ConflictPolicy::Overwrite,
            ..OutputSettings::default()
        };
        let error = ClaimedPaths::new()
            .resolve(&original, original.clone(), &overwrite)
            .unwrap_err();
        assert_eq!(error.code(), "would_overwrite_original");

        let allowed = OutputSettings {
            allow_overwrite_originals: true,
            ..overwrite
        };
        assert_eq!(
            ClaimedPaths::new()
                .resolve(&original, original.clone(), &allowed)
                .unwrap(),
            Resolution::Replace(original.clone())
        );
        // Renaming never touches the original.
        let rename = OutputSettings::default();
        let resolution = ClaimedPaths::new()
            .resolve(&original, original.clone(), &rename)
            .unwrap();
        assert_ne!(resolution.path(), original.as_path());
    }

    #[test]
    fn two_inputs_never_claim_the_same_output() {
        let dir = tempfile::tempdir().unwrap();
        let wanted = dir.path().join("a.webp");
        let claims = ClaimedPaths::new();
        let first = claims
            .resolve(
                Path::new("a.png"),
                wanted.clone(),
                &OutputSettings::default(),
            )
            .unwrap();
        let overwrite = OutputSettings {
            conflict: ConflictPolicy::Overwrite,
            ..OutputSettings::default()
        };
        let second = claims
            .resolve(Path::new("a.jpg"), wanted.clone(), &overwrite)
            .unwrap();
        assert_eq!(first, Resolution::Create(wanted.clone()));
        assert_eq!(second, Resolution::Create(dir.path().join("a (1).webp")));
    }
}
