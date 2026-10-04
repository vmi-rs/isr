use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use dpp::{
    DppError, apfs,
    apfs::ApfsVolume,
    hfsplus,
    hfsplus::HfsVolume,
    pbzx::PbzxReader,
    udif::{DmgArchive, DmgReaderOptions, PartitionInfo, PartitionType},
    xara::PkgReader,
};
use isr_dl::ProgressFn;
use sha2::{Digest, Sha256};

use crate::{DownloaderError, KernelCpioSink, KernelFiles};

/// Conventional path of the installer package inside the disk image.
const PACKAGE_PATH: &str = "/KernelDebugKit.pkg";

/// Conventional name of the package component carrying the KDK payload.
const COMPONENT: &str = "KDK.pkg";

/// Extracts `kernel_name` and its dSYM DWARF file from the KDK disk image at
/// `dmg_path`.
///
/// The filesystem partition and the package payload are spooled to temporary
/// files in `temp_directory`, and the decompressed cpio stream is parsed on
/// the fly, so memory use stays bounded regardless of the image size.
pub fn extract_kernel_files(
    dmg_path: &Path,
    kernel_name: &str,
    files: KernelFiles,
    temp_directory: &Path,
    progress: Option<ProgressFn>,
) -> Result<KernelFiles, DownloaderError> {
    std::fs::create_dir_all(temp_directory)?;

    let mut sink = KernelCpioSink::new(kernel_name, files.kernel, files.dsym, progress);
    stream_payload(dmg_path, temp_directory, &mut sink)?;
    sink.finish()
}

/// Streams the decompressed KDK package payload of the disk image at
/// `dmg_path` into `sink`.
fn stream_payload(
    dmg_path: &Path,
    temp_directory: &Path,
    sink: &mut impl Write,
) -> Result<(), DownloaderError> {
    tracing::debug!(path = %dmg_path.display(), "opening disk image");

    // UDIF checksum verification reads the whole data fork into memory, so it
    // is disabled. Downloads are verified against the manifest SHA-256 instead.
    let options = DmgReaderOptions {
        verify_checksums: false,
    };
    let mut archive = DmgArchive::open_with_options(dmg_path, options).map_err(DppError::from)?;
    let partitions = archive.partitions();
    let hfs = largest_partition(&partitions, |partition_type| {
        matches!(partition_type, PartitionType::Hfs | PartitionType::Hfsx)
    });
    let apfs = largest_partition(&partitions, |partition_type| {
        matches!(partition_type, PartitionType::Apfs)
    });

    let (partition, is_apfs) = match (hfs, apfs) {
        (Some(hfs), _) => (hfs, false),
        (None, Some(apfs)) => (apfs, true),
        (None, None) => return Err(DppError::NoFilesystemPartition.into()),
    };

    tracing::debug!(partition = %partition.name, "spooling filesystem partition");
    let mut partition_file = tempfile::tempfile_in(temp_directory)?;
    let mut writer = BufWriter::new(&mut partition_file);
    archive
        .extract_partition_to(partition.id, &mut writer)
        .map_err(DppError::from)?;
    writer.flush()?;
    drop(writer);
    partition_file.seek(SeekFrom::Start(0))?;
    let reader = BufReader::new(partition_file);

    if is_apfs {
        let mut volume = ApfsVolume::open(reader).map_err(DppError::from)?;
        let package_path = if volume.exists(PACKAGE_PATH).map_err(DppError::from)? {
            PACKAGE_PATH.to_owned()
        }
        else {
            let walk = volume.walk().map_err(DppError::from)?;
            find_package(
                walk.iter()
                    .filter(|walk_entry| matches!(walk_entry.entry.kind, apfs::EntryKind::File))
                    .map(|walk_entry| walk_entry.path.as_str()),
            )?
        };

        let package = volume.open_file(&package_path).map_err(DppError::from)?;
        stream_package(package, temp_directory, sink)
    }
    else {
        let mut volume = HfsVolume::open(reader).map_err(DppError::from)?;
        let package_path = if volume.exists(PACKAGE_PATH).map_err(DppError::from)? {
            PACKAGE_PATH.to_owned()
        }
        else {
            let walk = volume.walk().map_err(DppError::from)?;
            find_package(
                walk.iter()
                    .filter(|walk_entry| matches!(walk_entry.entry.kind, hfsplus::EntryKind::File))
                    .map(|walk_entry| walk_entry.path.as_str()),
            )?
        };

        let package = volume.open_file(&package_path).map_err(DppError::from)?;
        stream_package(package, temp_directory, sink)
    }
}

/// Streams the decompressed KDK payload of the installer package read from
/// `package` into `sink`.
fn stream_package(
    package: impl Read + Seek,
    temp_directory: &Path,
    sink: &mut impl Write,
) -> Result<(), DownloaderError> {
    let mut package = PkgReader::open(BufReader::new(package))?;
    let components = package.components();
    let component = match components.iter().find(|component| *component == COMPONENT) {
        Some(component) => component.as_str(),
        None => match components.first() {
            Some(component) => component.as_str(),
            None => "",
        },
    };

    tracing::debug!(component, "spooling package payload");
    let mut payload = tempfile::tempfile_in(temp_directory)?;
    let mut writer = BufWriter::new(&mut payload);
    package.payload_to(component, &mut writer)?;
    writer.flush()?;
    drop(writer);
    payload.seek(SeekFrom::Start(0))?;

    tracing::debug!("decompressing package payload");
    PbzxReader::new(BufReader::new(payload))?.decompress_to(sink)?;
    Ok(())
}

/// Returns the largest partition whose type satisfies `predicate`.
fn largest_partition(
    partitions: &[PartitionInfo],
    predicate: impl Fn(&PartitionType) -> bool,
) -> Option<&PartitionInfo> {
    partitions
        .iter()
        .filter(|partition| predicate(&partition.partition_type))
        .max_by_key(|partition| partition.size)
}

/// Picks the KDK installer package among the file `paths` of a volume.
fn find_package<'a>(paths: impl Iterator<Item = &'a str>) -> Result<String, DownloaderError> {
    let packages = paths
        .filter(|path| path.ends_with(".pkg"))
        .collect::<Vec<_>>();

    let package = match packages.iter().find(|path| path.contains("KernelDebugKit")) {
        Some(package) => *package,
        None => match packages.first() {
            Some(package) => *package,
            None => return Err(DownloaderError::PackageNotFound),
        },
    };

    if package.starts_with('/') {
        Ok(package.to_owned())
    }
    else {
        Ok(format!("/{package}"))
    }
}

/// Computes the lowercase hex SHA-256 digest of the file at `path`.
pub fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let mut digest = String::with_capacity(64);
    for byte in hasher.finalize() {
        write_hex(&mut digest, byte);
    }

    Ok(digest)
}

/// Appends the two lowercase hex digits of `byte` to `output`.
fn write_hex(output: &mut String, byte: u8) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    output.push(DIGITS[usize::from(byte >> 4)] as char);
    output.push(DIGITS[usize::from(byte & 0xf)] as char);
}

/// Verifies that the file at `path` has the lowercase hex SHA-256 `expected`.
pub fn verify_sha256(path: &Path, expected: &str) -> Result<(), DownloaderError> {
    tracing::debug!(path = %path.display(), "verifying SHA-256");
    let actual = sha256_file(path)?;

    if !actual.eq_ignore_ascii_case(expected) {
        return Err(DownloaderError::ChecksumMismatch {
            expected: expected.to_owned(),
            actual,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_known_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc");
        std::fs::write(&path, b"abc").unwrap();

        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(verify_sha256(&path, "00").is_err());
    }
}
