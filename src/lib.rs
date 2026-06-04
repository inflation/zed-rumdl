use std::{fs, path::PathBuf};

use zed_extension_api::{
    self as zed, Extension, LanguageServerId, Worktree, register_extension, settings::LspSettings,
};

pub struct Rumdl {
    binary_cache: Option<PathBuf>,
}

#[derive(Clone)]
struct RumdlBinary {
    path: PathBuf,
    env: Option<Vec<(String, String)>>,
}

const NAME: &str = "rumdl";

impl Rumdl {
    fn new() -> Self {
        Rumdl { binary_cache: None }
    }

    fn get_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<RumdlBinary> {
        // A user-configured binary path takes precedence over PATH and downloads.
        if let Some(path) = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.binary)
            .and_then(|binary| binary.path)
            .filter(|path| !path.is_empty())
        {
            return Ok(RumdlBinary {
                path: PathBuf::from(path),
                env: Some(worktree.shell_env()),
            });
        }

        if let Some(path) = worktree.which(NAME) {
            return Ok(RumdlBinary {
                path: PathBuf::from(path),
                env: Some(worktree.shell_env()),
            });
        }

        if let Some(path) = &self.binary_cache
            && path.exists()
        {
            return Ok(RumdlBinary {
                path: path.clone(),
                env: None,
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
            "rvben/rumdl",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )
        .map_err(|e| format!("Failed to fetch latest release: {e}"))?;

        let (platform, arch) = zed::current_platform();
        let arch_name = match arch {
            zed::Architecture::X8664 => "x86_64",
            zed::Architecture::Aarch64 => "aarch64",
            a => return Err(format!("Unsupported architecture: {a:?}")),
        };

        let (os_str, file_ext) = match platform {
            zed::Os::Mac => ("apple-darwin", "tar.gz"),
            zed::Os::Linux => ("unknown-linux-gnu", "tar.gz"),
            zed::Os::Windows => ("pc-windows-msvc", "zip"),
        };

        let asset_name = format!("{arch_name}-{os_str}.{file_ext}");
        let asset = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(&asset_name))
            .ok_or_else(|| format!("No compatible Rumdl binary found for {arch_name}-{os_str}"))?;

        let version_dir = format!("{NAME}-{}", release.version);
        let mut binary_path = PathBuf::from(&version_dir).join(NAME);

        if platform == zed::Os::Windows {
            binary_path.set_extension("exe");
        }

        if !binary_path.exists() {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            let download_result = (|| -> zed::Result<()> {
                zed::download_file(
                    &asset.download_url,
                    &version_dir,
                    if platform == zed::Os::Windows {
                        zed::DownloadedFileType::Zip
                    } else {
                        zed::DownloadedFileType::GzipTar
                    },
                )
                .map_err(|e| format!("Failed to download Rumdl binary: {e}"))?;

                zed::make_file_executable(binary_path.to_str().ok_or("Invalid binary path")?)
                    .map_err(|e| format!("Failed to make binary executable: {e}"))?;

                Ok(())
            })();

            if let Err(e) = download_result {
                fs::remove_dir_all(&version_dir).ok();
                return Err(e);
            }

            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string()
                        && name != version_dir
                    {
                        fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }
        }

        self.binary_cache = Some(binary_path.clone());
        Ok(RumdlBinary {
            path: binary_path,
            env: None,
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
            env: binary.env.unwrap_or_default(),
        })
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<Option<zed::serde_json::Value>> {
        let settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone());
        Ok(settings)
    }
}

register_extension!(Rumdl);
