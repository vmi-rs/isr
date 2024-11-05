use indexmap::IndexMap;
use url::Url;

use super::{
    error::Error,
    repository::{self, UbuntuRepositoryEntry},
};

pub struct UbuntuPackageCache {
    host: Url,
    packages: IndexMap<String, IndexMap<String, UbuntuRepositoryEntry>>,
}

impl UbuntuPackageCache {
    pub fn fetch(
        host: Url,
        arch: &str,
        dists: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Self, Error> {
        let mut packages = IndexMap::<String, IndexMap<String, UbuntuRepositoryEntry>>::new();

        for dist in dists {
            let dist = dist.as_ref();

            let repository = repository::fetch(host.clone(), arch, dist)?;
            let packages = packages.entry(dist.to_owned()).or_default();

            for entry in repository {
                let package = match entry.package.as_deref() {
                    Some(package) => package,
                    // Ignore packages without a name.
                    None => continue,
                };

                packages.entry(package.into()).or_insert(entry);
            }
        }

        Ok(Self { host, packages })
    }

    pub fn find_package(
        &self,
        package: &str,
        version: &str,
    ) -> Result<Option<&UbuntuRepositoryEntry>, Error> {
        tracing::info!(package, version, "finding package");
        self.find(package, version, false)
    }

    pub fn find_dbgsym_package(
        &self,
        package: &str,
        version: &str,
    ) -> Result<Option<&UbuntuRepositoryEntry>, Error> {
        tracing::info!(package, version, "finding dbgsym package");
        self.find(package, version, true)
    }

    pub fn package_url(&self, entry: &UbuntuRepositoryEntry) -> Result<Url, Error> {
        match &entry.filename {
            Some(filename) => Ok(self.host.join(filename)?),
            None => Err(Error::PackageMissingFilename),
        }
    }

    fn find(
        &self,
        package: &str,
        version: &str,
        dbgsym: bool,
    ) -> Result<Option<&UbuntuRepositoryEntry>, Error> {
        let mut candidates = Vec::new();

        for (dist, packages) in &self.packages {
            let entry = match packages.get(package) {
                Some(entry) => entry,
                None => continue,
            };

            let entry_version = match &entry.version {
                Some(entry_version) => entry_version,
                None => continue,
            };

            if entry_version != version {
                continue;
            }

            if dbgsym {
                //
                // Some dbgsym packages have dependencies on the main package.
                // For example:
                //    Package: linux-image-6.8.0-40-generic-dbgsym
                //    Depends: linux-image-unsigned-6.8.0-40-generic-dbgsym
                //    Size: 20796
                //
                // The main dbgsym package should not depend on anything.
                //

                if entry.depends.is_some() {
                    continue;
                }
            };

            // let entry_filename = match &entry.filename {
            //     Some(entry_filename) => entry_filename,
            //     None => continue,
            // };

            candidates.push((dist.as_str(), entry));
        }

        let candidate = match candidates.pop() {
            Some(candidate) => candidate,
            None => return Ok(None),
        };

        if !candidates.is_empty() {
            let dists = std::iter::once(candidate.0)
                .chain(candidates.into_iter().map(|(dist, _)| dist))
                .collect::<Vec<_>>();

            tracing::error!(?dists, "multiple candidates found");
            return Err(Error::PackageMultipleCandidates);
        }

        Ok(Some(candidate.1))
    }
}
