use anyhow::{Context as _, Result};
use std::{
    env,
    path::{Path, PathBuf},
};

#[cfg(windows)]
const SINGBOX_FILE_NAME: &str = "sing-box.exe";
#[cfg(not(windows))]
const SINGBOX_FILE_NAME: &str = "sing-box";

/// Resolves the optional sidecar from packaged resources, a portable app directory, or PATH.
/// Packaged and portable locations take precedence so a global executable cannot silently
/// replace the version shipped with clash5.
pub fn resolve() -> Result<PathBuf> {
    let resource_dir = crate::utils::dirs::app_resources_dir().ok();
    let executable_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let path_dirs: Vec<PathBuf> = env::var_os("PATH")
        .as_deref()
        .map(env::split_paths)
        .into_iter()
        .flatten()
        .collect();

    resolve_from(
        resource_dir.as_deref(),
        executable_dir.as_deref(),
        &path_dirs,
        SINGBOX_FILE_NAME,
    )
}

fn resolve_from(
    resource_dir: Option<&Path>,
    executable_dir: Option<&Path>,
    path_dirs: &[PathBuf],
    file_name: &str,
) -> Result<PathBuf> {
    let bundled = resource_dir.map(|dir| dir.join(file_name));
    let portable = executable_dir.map(|dir| dir.join(file_name));
    bundled
        .into_iter()
        .chain(portable)
        .chain(path_dirs.iter().map(|dir| dir.join(file_name)))
        .find(|path| path.is_file())
        .with_context(|| {
            format!("sing-box was not found in application resources, beside clash5, or on PATH ({file_name})")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Result<Self> {
            let root = env::temp_dir().join(format!("clash5-singbox-{}", nanoid::nanoid!()));
            fs::create_dir_all(&root).context("create sing-box resolution fixture")?;
            Ok(Self(root))
        }

        fn binary(&self, directory: &str, file_name: &str) -> Result<PathBuf> {
            let directory = self.0.join(directory);
            fs::create_dir_all(&directory).context("create candidate directory")?;
            let binary = directory.join(file_name);
            fs::write(&binary, b"fixture").context("create candidate binary")?;
            Ok(binary)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn packaged_binary_precedes_portable_and_path_versions() -> Result<()> {
        let fixture = Fixture::new()?;
        let packaged = fixture.binary("resources", SINGBOX_FILE_NAME)?;
        fixture.binary("portable", SINGBOX_FILE_NAME)?;
        fixture.binary("global-bin", SINGBOX_FILE_NAME)?;

        let resolved = resolve_from(
            Some(&fixture.0.join("resources")),
            Some(&fixture.0.join("portable")),
            &[fixture.0.join("global-bin")],
            SINGBOX_FILE_NAME,
        )?;

        assert_eq!(resolved, packaged);
        Ok(())
    }

    #[test]
    fn portable_and_path_fallbacks_work_when_no_packaged_binary_exists() -> Result<()> {
        let fixture = Fixture::new()?;
        let portable = fixture.binary("portable", SINGBOX_FILE_NAME)?;
        let global = fixture.binary("global-bin", SINGBOX_FILE_NAME)?;
        let resources = fixture.0.join("resources");

        assert_eq!(
            resolve_from(
                Some(&resources),
                Some(&fixture.0.join("portable")),
                &[],
                SINGBOX_FILE_NAME
            )?,
            portable
        );
        assert_eq!(
            resolve_from(
                Some(&resources),
                None,
                &[fixture.0.join("global-bin")],
                SINGBOX_FILE_NAME
            )?,
            global
        );
        Ok(())
    }

    #[test]
    fn missing_binary_reports_all_supported_locations() -> Result<()> {
        let fixture = Fixture::new()?;
        let error = match resolve_from(
            Some(&fixture.0.join("resources")),
            Some(&fixture.0.join("portable")),
            &[],
            SINGBOX_FILE_NAME,
        ) {
            Err(error) => error,
            Ok(path) => anyhow::bail!("missing executable unexpectedly resolved to {}", path.display()),
        };

        assert!(error.to_string().contains("application resources"));
        assert!(error.to_string().contains("PATH"));
        Ok(())
    }
}
