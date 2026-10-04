<!-- readme start -->
macOS specific downloaders and utilities.

Resolves an XNU kernel version string (e.g. `Darwin Kernel Version 27.0.0:
...; root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE`) to a Kernel Debug Kit
through the dortania `KdkSupportPkg` manifest, downloads the KDK disk image
and extracts the matching kernel binary and its dSYM. Installed KDK bundles
and local disk images can be used instead of a download.
<!-- readme end -->
