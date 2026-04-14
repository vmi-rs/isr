# Examples

- **[`windows.rs`]**

  Downloads a Windows PDB via `IsrCache`, then prints symbol addresses and
  struct offsets resolved from the profile. With a path argument, extracts
  the `CodeView` + `ImageSignature` from the PE and downloads it; without
  one, falls back to a hardcoded `CodeView` for the Windows 10.0.18362.356
  kernel.

  Run it with:

  ```
  cargo run --example windows --features cache,pdb,dl-windows [-- <image_path>]
  ```

- **[`ubuntu.rs`]**

  Downloads an Ubuntu kernel + debug symbols via `IsrCache` (hardcoded banner
  for the Ubuntu 6.8.0-40.40~22.04.3-generic kernel), then prints symbol
  addresses and struct offsets resolved from the profile. Wires up an
  `indicatif` multi-progress for download and extraction.

  Run it with:

  ```
  cargo run --example ubuntu --features cache,dwarf,dl-linux
  ```


[`windows.rs`]: https://github.com/vmi-rs/isr/blob/master/examples/windows.rs
[`ubuntu.rs`]: https://github.com/vmi-rs/isr/blob/master/examples/ubuntu.rs
