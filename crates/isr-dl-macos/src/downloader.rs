use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use bon::Builder;
use isr_dl::{Error, ProgressFn};
use reqwest::blocking::Client;
use url::Url;

use crate::{
    DarwinVersion, DownloaderError, KdkManifest, KernelFiles,
    dmg::{extract_kernel_files, verify_sha256},
};

/// Default KDK manifest maintained by the dortania `KdkSupportPkg` project.
pub const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/dortania/KdkSupportPkg/gh-pages/manifest.json";

/// Paths of a resolved Kernel Debug Kit kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KdkPaths {
    /// Directory holding the kernel files.
    pub output_directory: PathBuf,

    /// macOS build identifier of the KDK, e.g. `26A434`.
    pub build: String,

    /// Kernel file name, e.g. `kernel.release.vmapple`.
    pub kernel_name: String,

    /// Kernel binary.
    pub kernel: PathBuf,

    /// DWARF file inside the kernel dSYM bundle.
    pub dsym: PathBuf,

    /// KDK disk image the files were extracted from, if any.
    pub dmg: Option<PathBuf>,
}

/// Downloads macOS Kernel Debug Kits and extracts XNU kernels with their
/// dSYMs.
///
/// Extracted files are stored as
/// `<output_directory>/<build>/{<dmg>, <kernel_name>, <kernel_name>.dSYM/Contents/Resources/DWARF/<kernel_name>}`
/// and the manifest is cached at `<output_directory>/_index/manifest.json`.
#[derive(Builder)]
pub struct KdkSymbolDownloader {
    /// Lazily loaded manifest.
    #[builder(field)]
    manifest: OnceCell<KdkManifest>,

    /// HTTP client.
    #[builder(default)]
    client: Client,

    /// URL of the KDK manifest.
    #[builder(default = Url::parse(DEFAULT_MANIFEST_URL).unwrap())]
    manifest_url: Url,

    /// Directory holding the cached manifest and extracted kernels.
    #[builder(into)]
    output_directory: PathBuf,

    /// Local directories searched for installed `*.kdk` bundles, e.g.
    /// `/Library/Developer/KDKs`. A directory that is itself a KDK bundle is
    /// accepted as well.
    #[builder(
        default,
        with = |iter: impl IntoIterator<Item = impl Into<PathBuf>>| {
            iter.into_iter().map(Into::into).collect()
        }
    )]
    kdk_directories: Vec<PathBuf>,

    /// Optional progress callback.
    progress: Option<ProgressFn>,

    /// Maximum age of the cached manifest before re-fetching.
    /// `Duration::ZERO` always re-fetches; `Duration::MAX` always uses cache.
    #[builder(default = Duration::from_secs(24 * 3600))]
    index_max_age: Duration,
}

impl KdkSymbolDownloader {
    /// Returns the output directory.
    pub fn output_directory(&self) -> &Path {
        &self.output_directory
    }

    /// Pure filesystem lookup.
    ///
    /// Returns kernel files already extracted for `version` (resolved through
    /// the cached manifest), or files of a matching installed KDK bundle in
    /// one of the configured local directories. Never touches the network.
    pub fn lookup(&self, version: &DarwinVersion) -> Option<KdkPaths> {
        self.lookup_extracted(version)
            .or_else(|| self.lookup_installed(version))
    }

    /// Downloads the KDK listed for `version` in the manifest and extracts
    /// its kernel files.
    ///
    /// Already extracted files are reused. A disk image already present at
    /// the target path skips the download, but its SHA-256 is still verified
    /// against the manifest.
    pub fn download(&self, version: &DarwinVersion) -> Result<KdkPaths, Error> {
        Ok(self.download_inner(version)?)
    }

    /// Extracts the kernel files for `version` from a caller-provided local
    /// KDK disk image.
    ///
    /// The KDK build is taken from the cached manifest, or from the
    /// `_build_<build>.dmg` suffix of the disk image file name. Never touches
    /// the network.
    ///
    /// Before extraction, the disk image is verified against the SHA-256 of
    /// the manifest entry when one is listed. Extracted files whose kernel
    /// does not embed `version` are removed and rejected with
    /// [`DownloaderError::KernelVersionMismatch`].
    pub fn extract_from_dmg(
        &self,
        version: &DarwinVersion,
        dmg_path: impl AsRef<Path>,
    ) -> Result<KdkPaths, Error> {
        Ok(self.extract_from_dmg_inner(version, dmg_path.as_ref())?)
    }

    /// Implements [`Self::download`] with the crate error type.
    fn download_inner(&self, version: &DarwinVersion) -> Result<KdkPaths, DownloaderError> {
        let manifest = self.fetch_manifest()?;
        let found = manifest
            .resolve(version)
            .ok_or(DownloaderError::KernelNotFound)?;

        let mut paths = self.paths(&found.entry.build, found.kernel_name);
        if paths.kernel.exists() && paths.dsym.exists() {
            return Ok(paths);
        }

        let url = Url::parse(&found.entry.url)?;
        let dmg_name = match url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
        {
            Some(dmg_name) if !dmg_name.is_empty() => dmg_name.to_owned(),
            _ => format!("{}.dmg", found.entry.build),
        };
        let dmg_path = paths.output_directory.join(dmg_name);

        if dmg_path.exists() {
            tracing::debug!(path = %dmg_path.display(), "reusing disk image");
            if let Some(sha256sum) = &found.entry.sha256sum {
                verify_sha256(&dmg_path, sha256sum)?;
            }
        }
        else {
            std::fs::create_dir_all(&paths.output_directory)?;
            self.fetch(&url, &dmg_path)?;

            if let Some(sha256sum) = &found.entry.sha256sum
                && let Err(err) = verify_sha256(&dmg_path, sha256sum)
            {
                std::fs::remove_file(&dmg_path)?;
                return Err(err);
            }
        }

        self.extract(&paths, &dmg_path)?;
        paths.dmg = Some(dmg_path);
        Ok(paths)
    }

    /// Implements [`Self::extract_from_dmg`] with the crate error type.
    fn extract_from_dmg_inner(
        &self,
        version: &DarwinVersion,
        dmg_path: &Path,
    ) -> Result<KdkPaths, DownloaderError> {
        let manifest = self.load_cached_manifest();
        let found = manifest
            .as_ref()
            .and_then(|manifest| manifest.resolve(version));

        let (build, kernel_name, sha256sum) = match found {
            Some(found) => (
                found.entry.build.clone(),
                found.kernel_name.to_owned(),
                found.entry.sha256sum.as_deref(),
            ),
            None => match build_from_dmg_name(dmg_path) {
                Some(build) => (build, version.kernel_name(), None),
                None => return Err(DownloaderError::UnknownBuild),
            },
        };

        let mut paths = self.paths(&build, &kernel_name);
        if !paths.kernel.exists() || !paths.dsym.exists() {
            if let Some(sha256sum) = sha256sum {
                verify_sha256(dmg_path, sha256sum)?;
            }

            self.extract(&paths, dmg_path)?;

            if !file_contains(&paths.kernel, version.original.as_bytes()) {
                std::fs::remove_file(&paths.kernel)?;
                std::fs::remove_dir_all(dsym_bundle_path(
                    &paths.output_directory,
                    &paths.kernel_name,
                ))?;
                return Err(DownloaderError::KernelVersionMismatch);
            }
        }

        paths.dmg = Some(dmg_path.to_owned());
        Ok(paths)
    }

    /// Extracts the kernel files described by `paths` from `dmg_path`.
    fn extract(&self, paths: &KdkPaths, dmg_path: &Path) -> Result<(), DownloaderError> {
        extract_kernel_files(
            dmg_path,
            &paths.kernel_name,
            KernelFiles {
                kernel: paths.kernel.clone(),
                dsym: paths.dsym.clone(),
            },
            &paths.output_directory,
            self.progress.clone(),
        )?;

        Ok(())
    }

    /// Returns the extraction paths for `kernel_name` of KDK `build`.
    fn paths(&self, build: &str, kernel_name: &str) -> KdkPaths {
        let output_directory = self.output_directory.join(build);
        let kernel = output_directory.join(kernel_name);
        let dsym = dsym_path(&output_directory, kernel_name);

        KdkPaths {
            output_directory,
            build: build.to_owned(),
            kernel_name: kernel_name.to_owned(),
            kernel,
            dsym,
            dmg: None,
        }
    }

    /// Looks up previously extracted files through the cached manifest.
    fn lookup_extracted(&self, version: &DarwinVersion) -> Option<KdkPaths> {
        let manifest = self.load_cached_manifest()?;
        let found = manifest.resolve(version)?;
        let paths = self.paths(&found.entry.build, found.kernel_name);

        if !paths.kernel.exists() || !paths.dsym.exists() {
            return None;
        }

        Some(paths)
    }

    /// Looks up an installed KDK bundle whose kernel embeds `version`.
    fn lookup_installed(&self, version: &DarwinVersion) -> Option<KdkPaths> {
        let kernel_name = version.kernel_name();

        self.kdk_directories
            .iter()
            .flat_map(|directory| kdk_bundles(directory))
            .find_map(|bundle| {
                let kernels = bundle.join("System").join("Library").join("Kernels");
                let kernel = kernels.join(&kernel_name);
                let dsym = dsym_path(&kernels, &kernel_name);

                if !dsym.exists() || !file_contains(&kernel, version.original.as_bytes()) {
                    return None;
                }

                let build = build_from_bundle_name(&bundle)?;
                tracing::debug!(path = %bundle.display(), "found installed KDK");

                Some(KdkPaths {
                    output_directory: kernels,
                    build,
                    kernel_name: kernel_name.clone(),
                    kernel,
                    dsym,
                    dmg: None,
                })
            })
    }

    /// Returns the manifest, fetching it when the cached copy is stale.
    fn fetch_manifest(&self) -> Result<&KdkManifest, DownloaderError> {
        if let Some(manifest) = self.manifest.get() {
            return Ok(manifest);
        }

        let path = self.manifest_path();
        if !is_fresh(&path, self.index_max_age) {
            std::fs::create_dir_all(self.index_directory())?;
            self.fetch(&self.manifest_url, &path)?;
        }

        let manifest = KdkManifest::from_json(&std::fs::read(&path)?)?;
        Ok(self.manifest.get_or_init(|| manifest))
    }

    /// Reads the cached manifest regardless of its age.
    ///
    /// The manifest is returned as an owned value and never stored in
    /// `self.manifest`, so a stale copy cannot bypass the freshness check of
    /// [`Self::fetch_manifest`].
    fn load_cached_manifest(&self) -> Option<KdkManifest> {
        let json = std::fs::read(self.manifest_path()).ok()?;
        match KdkManifest::from_json(&json) {
            Ok(manifest) => Some(manifest),
            Err(err) => {
                tracing::debug!(%err, "cached manifest unavailable");
                None
            }
        }
    }

    /// Downloads `url` to `output` through a `.part` file.
    fn fetch(&self, url: &Url, output: &Path) -> Result<(), DownloaderError> {
        tracing::debug!(%url, "downloading");
        let mut response = self.client.get(url.clone()).send()?.error_for_status()?;
        let total_bytes = response.content_length();

        isr_dl::stream_download(
            &mut response,
            output,
            url,
            total_bytes,
            self.progress.clone(),
        )?;

        Ok(())
    }

    /// Returns the directory holding the cached manifest.
    fn index_directory(&self) -> PathBuf {
        self.output_directory.join("_index")
    }

    /// Returns the path of the cached manifest.
    fn manifest_path(&self) -> PathBuf {
        self.index_directory().join("manifest.json")
    }
}

/// Returns the dSYM bundle directory of `kernel_name` in `directory`.
fn dsym_bundle_path(directory: &Path, kernel_name: &str) -> PathBuf {
    directory.join(format!("{kernel_name}.dSYM"))
}

/// Returns the DWARF path inside the dSYM bundle of `kernel_name` in
/// `directory`.
fn dsym_path(directory: &Path, kernel_name: &str) -> PathBuf {
    dsym_bundle_path(directory, kernel_name)
        .join("Contents")
        .join("Resources")
        .join("DWARF")
        .join(kernel_name)
}

/// Returns `directory` if it is a KDK bundle, otherwise the `*.kdk` bundles
/// it contains.
fn kdk_bundles(directory: &Path) -> Vec<PathBuf> {
    if directory
        .join("System")
        .join("Library")
        .join("Kernels")
        .is_dir()
    {
        return vec![directory.to_owned()];
    }

    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::debug!(path = %directory.display(), %err, "KDK directory unavailable");
            return Vec::new();
        }
    };

    let mut bundles = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "kdk"))
        .collect::<Vec<_>>();
    bundles.sort();
    bundles
}

/// Extracts the build from a `KDK_<version>_<build>.kdk` bundle name.
fn build_from_bundle_name(bundle: &Path) -> Option<String> {
    let stem = bundle.file_stem()?.to_str()?;
    let build = stem.rsplit('_').next()?;
    (!build.is_empty()).then(|| build.to_owned())
}

/// Extracts the build from a `..._build_<build>.dmg` disk image name.
fn build_from_dmg_name(dmg_path: &Path) -> Option<String> {
    let stem = dmg_path.file_stem()?.to_str()?;
    let (_, build) = stem.rsplit_once("_build_")?;
    (!build.is_empty()).then(|| build.to_owned())
}

/// Returns whether the file at `path` contains `needle`.
fn file_contains(path: &Path, needle: &[u8]) -> bool {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(_) => return false,
    };

    !needle.is_empty() && data.windows(needle.len()).any(|window| window == needle)
}

/// Returns whether `path` exists and is younger than `max_age`.
fn is_fresh(path: &Path, max_age: Duration) -> bool {
    let mtime = match std::fs::metadata(path).and_then(|metadata| metadata.modified()) {
        Ok(mtime) => mtime,
        Err(_) => return false,
    };

    SystemTime::now()
        .duration_since(mtime)
        .map(|age| age < max_age)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::TcpListener,
        thread::JoinHandle,
    };

    use super::*;

    const VERSION: &str = "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; \
                           root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE";

    /// Creates a downloader that never refreshes the manifest.
    fn downloader(output_directory: &Path, kdk_directory: &Path) -> KdkSymbolDownloader {
        KdkSymbolDownloader::builder()
            .output_directory(output_directory)
            .kdk_directories([kdk_directory])
            .index_max_age(Duration::MAX)
            .build()
    }

    #[test]
    fn finds_installed_kdk_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let kernels = dir
            .path()
            .join("KDK_27.0.1_26A434.kdk")
            .join("System")
            .join("Library")
            .join("Kernels");
        let dsym = dsym_path(&kernels, "kernel.release.vmapple");
        std::fs::create_dir_all(dsym.parent().unwrap()).unwrap();
        std::fs::write(&dsym, b"dwarf").unwrap();
        std::fs::write(
            kernels.join("kernel.release.vmapple"),
            format!("\0\0{VERSION}\0\0"),
        )
        .unwrap();

        let version = VERSION.parse::<DarwinVersion>().unwrap();
        let downloader = downloader(&dir.path().join("out"), dir.path());
        let paths = downloader.lookup(&version).unwrap();
        assert_eq!(paths.build, "26A434");
        assert_eq!(paths.kernel, kernels.join("kernel.release.vmapple"));
        assert_eq!(paths.dsym, dsym);

        // A kernel built from a different version string is ignored.
        let other = VERSION.replace("21:00:07", "21:00:08");
        let other = other.parse::<DarwinVersion>().unwrap();
        assert!(downloader.lookup(&other).is_none());
    }

    #[test]
    fn finds_extracted_kernel_through_cached_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let manifest = format!(
            r#"[{{"build": "26A434", "version": "27.0.1", "url": "https://example.com/k.dmg",
                 "kernel_versions": {{"kernel.release.vmapple": "{VERSION}"}}}}]"#
        );
        std::fs::create_dir_all(out.join("_index")).unwrap();
        std::fs::write(out.join("_index").join("manifest.json"), manifest).unwrap();

        let version = VERSION.parse::<DarwinVersion>().unwrap();
        let downloader = downloader(&out, &dir.path().join("none"));
        assert!(downloader.lookup(&version).is_none());

        let build = out.join("26A434");
        let dsym = dsym_path(&build, "kernel.release.vmapple");
        std::fs::create_dir_all(dsym.parent().unwrap()).unwrap();
        std::fs::write(&dsym, b"dwarf").unwrap();
        std::fs::write(build.join("kernel.release.vmapple"), b"kernel").unwrap();

        let paths = downloader.lookup(&version).unwrap();
        assert_eq!(paths.output_directory, build);
        assert_eq!(paths.kernel_name, "kernel.release.vmapple");
    }

    #[test]
    fn parses_build_from_names() {
        assert_eq!(
            build_from_dmg_name(Path::new("Kernel_Debug_Kit_27.0.1_build_26A434.dmg")).as_deref(),
            Some("26A434")
        );
        assert_eq!(build_from_dmg_name(Path::new("kdk.dmg")), None);
        assert_eq!(
            build_from_bundle_name(Path::new("KDK_27.0.1_26A434.kdk")).as_deref(),
            Some("26A434")
        );
    }

    /// Returns a manifest listing `VERSION` under `build`.
    fn manifest_json(build: &str, sha256sum: &str) -> String {
        format!(
            r#"[{{"build": "{build}", "version": "27.0.1", "url": "https://example.com/k.dmg",
                 "sha256sum": "{sha256sum}",
                 "kernel_versions": {{"kernel.release.vmapple": "{VERSION}"}}}}]"#
        )
    }

    /// Creates extracted kernel files for `kernel_name` in `directory`.
    fn create_kernel_files(directory: &Path, kernel_name: &str) {
        let dsym = dsym_path(directory, kernel_name);
        std::fs::create_dir_all(dsym.parent().unwrap()).unwrap();
        std::fs::write(&dsym, b"dwarf").unwrap();
        std::fs::write(directory.join(kernel_name), b"kernel").unwrap();
    }

    /// Serves `body` to a single HTTP request and returns the URL serving it.
    fn serve_once(body: String) -> (Url, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!(
            "http://{}/manifest.json",
            listener.local_addr().unwrap()
        ))
        .unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
            }

            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });

        (url, handle)
    }

    #[test]
    fn stale_manifest_read_by_lookup_is_refreshed_by_download() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(out.join("_index")).unwrap();
        std::fs::write(out.join("_index").join("manifest.json"), "[]").unwrap();
        create_kernel_files(&out.join("26A434"), "kernel.release.vmapple");

        let (manifest_url, server) = serve_once(manifest_json("26A434", ""));
        let downloader = KdkSymbolDownloader::builder()
            .client(Client::builder().no_proxy().build().unwrap())
            .manifest_url(manifest_url)
            .output_directory(&out)
            .index_max_age(Duration::ZERO)
            .build();

        // The stale cached manifest does not list the kernel yet.
        let version = VERSION.parse::<DarwinVersion>().unwrap();
        assert!(downloader.lookup(&version).is_none());

        let paths = downloader.download(&version).unwrap();
        server.join().unwrap();
        assert_eq!(paths.build, "26A434");
        assert_eq!(paths.output_directory, out.join("26A434"));
    }

    #[test]
    fn rejects_local_disk_image_with_wrong_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let expected = "0".repeat(64);
        std::fs::create_dir_all(out.join("_index")).unwrap();
        std::fs::write(
            out.join("_index").join("manifest.json"),
            manifest_json("26A434", &expected),
        )
        .unwrap();
        let dmg_path = dir.path().join("kdk.dmg");
        std::fs::write(&dmg_path, b"not a disk image").unwrap();

        let version = VERSION.parse::<DarwinVersion>().unwrap();
        let downloader = downloader(&out, &dir.path().join("none"));
        let err = downloader
            .extract_from_dmg_inner(&version, &dmg_path)
            .unwrap_err();
        assert!(
            matches!(err, DownloaderError::ChecksumMismatch { expected: digest, .. } if digest == expected)
        );
        assert!(!out.join("26A434").join("kernel.release.vmapple").exists());
    }
}
