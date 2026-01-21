use std::fs;
use std::path::{Path, PathBuf};

use zed_extension_api::{
    self as zed, Extension, LanguageServerId, Worktree, register_extension, settings::LspSettings,
};

pub struct Rumdl {
    binary_cache: Option<PathBuf>,
}

#[derive(Clone)]
struct RumdlBinary {
    path: PathBuf,
    env: Vec<(String, String)>,
}

const NAME: &str = "rumdl";
const NAME_PREFIX: &str = "rumdl-";
const RUMDL_GITHUB_REPO: &str = "rvben/rumdl";

impl Rumdl {
    const fn new() -> Self {
        Self { binary_cache: None }
    }

    fn arch_name(arch: zed::Architecture) -> zed::Result<&'static str> {
        if arch == zed::Architecture::X8664 {
            return Ok("x86_64");
        }

        if arch == zed::Architecture::Aarch64 {
            return Ok("aarch64");
        }

        Err(format!("Unsupported architecture: {arch:?}"))
    }

    fn os_asset_info(platform: zed::Os) -> (&'static str, &'static str) {
        if platform == zed::Os::Mac {
            return ("apple-darwin", "tar.gz");
        }

        if platform == zed::Os::Linux {
            return ("unknown-linux-gnu", "tar.gz");
        }

        ("pc-windows-msvc", "zip")
    }

    fn find_release_asset<'a>(
        release: &'a zed::GithubRelease,
        arch_name: &str,
        os_str: &str,
        file_ext: &str,
    ) -> zed::Result<&'a zed::GithubReleaseAsset> {
        let asset_name = format!("{arch_name}-{os_str}.{file_ext}");
        let asset = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(&asset_name));

        let Some(asset) = asset else {
            return Err(format!(
                "No compatible Rumdl binary found for {arch_name}-{os_str}"
            ));
        };

        Ok(asset)
    }

    fn find_release_asset_for_platform<'a>(
        release: &'a zed::GithubRelease,
        arch_name: &str,
        platform: zed::Os,
    ) -> zed::Result<&'a zed::GithubReleaseAsset> {
        let (os_str, file_ext) = Self::os_asset_info(platform);
        if platform != zed::Os::Linux {
            return Self::find_release_asset(release, arch_name, os_str, file_ext);
        }

        let musl_asset =
            Self::find_release_asset(release, arch_name, "unknown-linux-musl", file_ext);
        if let Ok(asset) = musl_asset {
            return Ok(asset);
        }

        let musl_error = musl_asset
            .err()
            .unwrap_or_else(|| "unknown linux musl asset failure".into());

        let gnu_asset = Self::find_release_asset(release, arch_name, os_str, file_ext);
        if let Ok(asset) = gnu_asset {
            return Ok(asset);
        }

        let gnu_error = gnu_asset
            .err()
            .unwrap_or_else(|| "unknown linux gnu asset failure".into());

        Err(format!(
            "No compatible Rumdl binary found: musl attempt failed: {musl_error}; gnu attempt failed: {gnu_error}"
        ))
    }

    fn build_versioned_binary_path(
        release_version: &str,
        platform: zed::Os,
    ) -> zed::Result<(String, PathBuf)> {
        if release_version.contains('/') {
            return Err("Invalid release version: contains '/'".into());
        }

        if release_version.contains('\\') {
            return Err("Invalid release version: contains '\\\\'".into());
        }

        let version_dir = format!("{NAME}-{release_version}");
        let mut binary_path = PathBuf::from(&version_dir).join(NAME);

        if platform == zed::Os::Windows {
            binary_path.set_extension("exe");
        }

        Ok((version_dir, binary_path))
    }

    fn download_binary(
        language_server_id: &LanguageServerId,
        version_dir: &str,
        binary_path: &Path,
        asset: &zed::GithubReleaseAsset,
        platform: zed::Os,
    ) -> zed::Result<()> {
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        let file_type = match platform {
            zed::Os::Windows => zed::DownloadedFileType::Zip,
            _ => zed::DownloadedFileType::GzipTar,
        };

        zed::download_file(&asset.download_url, version_dir, file_type)
            .map_err(|e| format!("Failed to download Rumdl binary: {e}"))?;

        let binary_path = binary_path.to_str().ok_or("Invalid binary path")?;
        zed::make_file_executable(binary_path)
            .map_err(|e| format!("Failed to make binary executable: {e}"))?;

        Ok(())
    }

    fn download_binary_or_cleanup(
        language_server_id: &LanguageServerId,
        version_dir: &str,
        binary_path: &Path,
        asset: &zed::GithubReleaseAsset,
        platform: zed::Os,
    ) -> zed::Result<()> {
        let download_result = Self::download_binary(
            language_server_id,
            version_dir,
            binary_path,
            asset,
            platform,
        );
        if download_result.is_ok() {
            return Ok(());
        }

        // Best-effort cleanup to avoid leaving partial installs behind.
        fs::remove_dir_all(version_dir).ok();
        download_result
    }

    fn cleanup_other_versions(current_version_dir: &str) {
        let Ok(entries) = fs::read_dir(".") else {
            return;
        };

        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };

            if name == current_version_dir {
                continue;
            }

            if !name.starts_with(NAME_PREFIX) {
                continue;
            }

            fs::remove_dir_all(entry.path()).ok();
        }
    }

    fn get_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<RumdlBinary> {
        if let Some(path) = worktree.which(NAME) {
            return Ok(RumdlBinary {
                path: PathBuf::from(path),
                env: worktree.shell_env(),
            });
        }

        if let Some(path) = &self.binary_cache
            && path.exists()
        {
            return Ok(RumdlBinary {
                path: path.clone(),
                env: Vec::new(),
            });
        }

        self.install_binary(language_server_id)
    }

    fn install_binary(
        &mut self,
        language_server_id: &LanguageServerId,
    ) -> zed::Result<RumdlBinary> {
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            RUMDL_GITHUB_REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )
        .map_err(|e| format!("Failed to fetch latest release: {e}"))?;

        let (platform, arch) = zed::current_platform();
        let arch_name = Self::arch_name(arch)?;
        let asset = Self::find_release_asset_for_platform(&release, arch_name, platform)?;
        let (version_dir, binary_path) =
            Self::build_versioned_binary_path(&release.version, platform)?;

        if binary_path.exists() {
            self.binary_cache = Some(binary_path.clone());
            return Ok(RumdlBinary {
                path: binary_path,
                env: Vec::new(),
            });
        }

        Self::download_binary_or_cleanup(
            language_server_id,
            &version_dir,
            &binary_path,
            asset,
            platform,
        )?;
        Self::cleanup_other_versions(&version_dir);

        self.binary_cache = Some(binary_path.clone());
        Ok(RumdlBinary {
            path: binary_path,
            env: Vec::new(),
        })
    }
}

impl Extension for Rumdl {
    fn new() -> Self {
        Self::new()
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<zed::Command> {
        let binary = self.get_binary(language_server_id, worktree)?;
        Ok(zed::Command {
            command: binary
                .path
                .to_str()
                .ok_or("Failed to convert binary path to string")?
                .into(),
            args: vec!["server".into()],
            env: binary.env,
        })
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<Option<zed::serde_json::Value>> {
        let settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings);
        Ok(settings)
    }
}

register_extension!(Rumdl);
