//! Parsed package index for one Debian-style repository.

use indexmap::IndexMap;
use url::Url;

use super::{error::UbuntuError, parse::UbuntuRepositoryEntry};

/// Query parameters for looking up a package across all dists in an index.
#[derive(Debug, Clone)]
pub struct PackageQuery {
    /// Debian package name to match.
    pub package: String,

    /// Exact `Version:` field to match.
    pub version: String,

    /// If true, applies the dbgsym filter (no `Depends` field).
    pub dbgsym: bool,

    /// If true and the primary package name is not found, retries with the
    /// `linux-image-unsigned-*` form (the `linux-image-` prefix is replaced
    /// with `linux-image-unsigned-`).
    pub unsigned_fallback: bool,
}

/// Parsed package index for one repository host across multiple dists.
#[derive(Debug)]
pub struct PackageIndex {
    /// Base URL of the repository host + `Filename`.
    host: Url,

    /// `dist -> package_name -> entry`.
    ///
    /// Using `IndexMap` for stable iteration.
    packages: IndexMap<String, IndexMap<String, UbuntuRepositoryEntry>>,
}

impl PackageIndex {
    /// Creates a new `PackageIndex`.
    pub fn new(
        host: Url,
        packages: IndexMap<String, IndexMap<String, UbuntuRepositoryEntry>>,
    ) -> Self {
        Self { host, packages }
    }

    /// Returns the repository host URL for this index.
    pub fn host(&self) -> &Url {
        &self.host
    }

    /// Looks up a package matching the query across all dists.
    ///
    /// Returns the matching entry. Errors with `PackageMultipleCandidates` if
    /// more than one dist contains a matching entry.
    pub fn find(
        &self,
        query: &PackageQuery,
    ) -> Result<Option<&UbuntuRepositoryEntry>, UbuntuError> {
        if let Some(entry) = self.find_inner(&query.package, &query.version, query.dbgsym)? {
            return Ok(Some(entry));
        }

        if query.unsigned_fallback
            && let Some(unsigned) = unsigned_variant(&query.package)
        {
            return self.find_inner(&unsigned, &query.version, query.dbgsym);
        }

        Ok(None)
    }

    /// Resolves a package entry to its full download URL via this index's host.
    pub fn resolve_url(&self, entry: &UbuntuRepositoryEntry) -> Result<Url, UbuntuError> {
        match &entry.filename {
            Some(filename) => Ok(self.host.join(filename)?),
            None => Err(UbuntuError::PackageMissingFilename),
        }
    }

    fn find_inner(
        &self,
        package: &str,
        version: &str,
        dbgsym: bool,
    ) -> Result<Option<&UbuntuRepositoryEntry>, UbuntuError> {
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

            // dbgsym packages should not have a `Depends:` field. If they do,
            // they are wrapper packages that depend on the real dbgsym package.
            if dbgsym && entry.depends.is_some() {
                continue;
            }

            candidates.push((dist.as_str(), entry));
        }

        let candidate = match candidates.pop() {
            Some(candidate) => candidate,
            None => return Ok(None),
        };

        if !candidates.is_empty() {
            let dists = std::iter::once(candidate.0)
                .chain(candidates.into_iter().map(|(d, _)| d))
                .collect::<Vec<_>>();

            tracing::error!(?dists, "multiple candidates found");
            return Err(UbuntuError::PackageMultipleCandidates);
        }

        Ok(Some(candidate.1))
    }
}

/// Returns the `linux-image-unsigned-*` variant of a `linux-image-*` package
/// name, or `None` if the input is not a `linux-image-` name.
fn unsigned_variant(package: &str) -> Option<String> {
    package
        .strip_prefix("linux-image-")
        .map(|rest| format!("linux-image-unsigned-{rest}"))
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::ubuntu::parse::UbuntuRepositoryEntry;

    fn entry(package: &str, version: &str, filename: &str) -> UbuntuRepositoryEntry {
        UbuntuRepositoryEntry {
            package: Some(package.into()),
            version: Some(version.into()),
            filename: Some(filename.into()),
            ..Default::default()
        }
    }

    fn index_with(entries: Vec<(&str, Vec<UbuntuRepositoryEntry>)>) -> PackageIndex {
        let mut packages = IndexMap::new();
        for (dist, dist_entries) in entries {
            let mut map = IndexMap::new();
            for e in dist_entries {
                map.insert(e.package.clone().unwrap(), e);
            }
            packages.insert(dist.into(), map);
        }
        PackageIndex::new("http://example.com/ubuntu/".try_into().unwrap(), packages)
    }

    #[test]
    fn finds_signed_kernel() {
        let idx = index_with(vec![(
            "noble",
            vec![entry(
                "linux-image-6.8.0-40-generic",
                "6.8.0-40.40",
                "pool/x.deb",
            )],
        )]);
        let query = PackageQuery {
            package: "linux-image-6.8.0-40-generic".into(),
            version: "6.8.0-40.40".into(),
            dbgsym: false,
            unsigned_fallback: true,
        };
        let found = idx.find(&query).unwrap().unwrap();
        assert_eq!(found.filename.as_deref(), Some("pool/x.deb"));
    }

    #[test]
    fn falls_back_to_unsigned_when_signed_missing() {
        let idx = index_with(vec![(
            "noble",
            vec![entry(
                "linux-image-unsigned-6.8.0-40-generic",
                "6.8.0-40.40",
                "pool/u.deb",
            )],
        )]);
        let query = PackageQuery {
            package: "linux-image-6.8.0-40-generic".into(),
            version: "6.8.0-40.40".into(),
            dbgsym: false,
            unsigned_fallback: true,
        };
        let found = idx.find(&query).unwrap().unwrap();
        assert_eq!(found.filename.as_deref(), Some("pool/u.deb"));
    }

    #[test]
    fn unsigned_fallback_disabled_returns_none() {
        let idx = index_with(vec![(
            "noble",
            vec![entry(
                "linux-image-unsigned-6.8.0-40-generic",
                "6.8.0-40.40",
                "pool/u.deb",
            )],
        )]);
        let query = PackageQuery {
            package: "linux-image-6.8.0-40-generic".into(),
            version: "6.8.0-40.40".into(),
            dbgsym: false,
            unsigned_fallback: false,
        };
        assert!(idx.find(&query).unwrap().is_none());
    }

    #[test]
    fn dbgsym_filter_skips_packages_with_depends() {
        let mut wrapper = entry(
            "linux-image-6.8.0-40-generic-dbgsym",
            "6.8.0-40.40",
            "pool/wrapper.deb",
        );
        wrapper.depends = Some("linux-image-unsigned-6.8.0-40-generic-dbgsym".into());
        let real = entry(
            "linux-image-unsigned-6.8.0-40-generic-dbgsym",
            "6.8.0-40.40",
            "pool/real.deb",
        );

        let idx = index_with(vec![("noble", vec![wrapper, real])]);
        let query = PackageQuery {
            package: "linux-image-6.8.0-40-generic-dbgsym".into(),
            version: "6.8.0-40.40".into(),
            dbgsym: true,
            unsigned_fallback: true,
        };
        // The signed one has Depends -> filtered out -> falls back to unsigned.
        let found = idx.find(&query).unwrap().unwrap();
        assert_eq!(found.filename.as_deref(), Some("pool/real.deb"));
    }

    #[test]
    fn multiple_candidates_in_different_dists_errors() {
        let idx = index_with(vec![
            (
                "noble",
                vec![entry(
                    "linux-image-6.8.0-40-generic",
                    "6.8.0-40.40",
                    "pool/a.deb",
                )],
            ),
            (
                "noble-updates",
                vec![entry(
                    "linux-image-6.8.0-40-generic",
                    "6.8.0-40.40",
                    "pool/b.deb",
                )],
            ),
        ]);
        let query = PackageQuery {
            package: "linux-image-6.8.0-40-generic".into(),
            version: "6.8.0-40.40".into(),
            dbgsym: false,
            unsigned_fallback: false,
        };
        assert!(matches!(
            idx.find(&query),
            Err(UbuntuError::PackageMultipleCandidates)
        ));
    }

    #[test]
    fn resolve_url_joins_host_and_filename() {
        let idx = index_with(vec![(
            "noble",
            vec![entry("foo", "1.0", "pool/main/f/foo.deb")],
        )]);
        let entry = idx
            .find(&PackageQuery {
                package: "foo".into(),
                version: "1.0".into(),
                dbgsym: false,
                unsigned_fallback: false,
            })
            .unwrap()
            .unwrap();
        let url = idx.resolve_url(entry).unwrap();
        assert_eq!(
            url.as_str(),
            "http://example.com/ubuntu/pool/main/f/foo.deb"
        );
    }
}
