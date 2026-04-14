//! Downloads a Windows PDB via `IsrCache`, then prints symbol addresses and
//! struct offsets resolved from the profile.
//!
//! With an argument, extracts the `CodeView` + `ImageSignature` from the given
//! PE and downloads it. Without an argument, uses a hardcoded `CodeView` for
//! the Windows 10.0.18362.356 kernel.

use isr::{
    IsrCache,
    download::windows::{CodeView, ImageSignature},
    macros::{Bitfield, Field, offsets, symbols},
};

symbols! {
    #[derive(Debug)]
    pub struct Symbols {
        PsActiveProcessHead: u64,

        // Optional symbols might be missing from profile.
        PsInitialSystemProcess: Option<u64>,
        NonExistentSymbol: Option<u64>,

        // Provide aliases when symbols might have different names across builds.
        #[isr(alias = "KiSystemCall64Shadow")]
        KiSystemCall64: u64,

        // Multiple aliases for a symbol.
        #[isr(alias = ["_NtOpenFile@24", "NtOpenFile"])]
        NtOpenFile: u64,
    }
}

offsets! {
    // Defined attributes are applied to each substucture.
    #[derive(Debug)]
    pub struct Offsets {
        struct _EX_FAST_REF {
            RefCnt: Bitfield,
            Value: Field,
        }

        struct _EPROCESS {
            UniqueProcessId: Field,

            // Define an alternative name for a field.
            #[isr(alias = "Wow64Process")]
            WoW64Process: Field,

            // We can even define field names that are present
            // in the nested structures.
            Affinity: Field, // Nested, defined in _KPROCESS
        }

        // Define an alternative name for a structure.
        #[isr(alias = "_KLDR_DATA_TABLE_ENTRY")]
        struct _LDR_DATA_TABLE_ENTRY {
            InLoadOrderLinks: Field,
            DllBase: Field,
            FullDllName: Field,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = IsrCache::new("cache")?;

    let codeview = match std::env::args().nth(1) {
        Some(image_path) => {
            let codeview = CodeView::from_path(&image_path)?;
            let image_signature = ImageSignature::from_path(&image_path, codeview.filename())?;

            println!("CodeView:        {codeview:#?}");
            println!("ImageSignature:  {image_signature:#?}");

            // Not necessary, but demonstrates downloading an image from
            // a Microsoft symbol server using the image signature.
            cache.download_from_image_signature(image_signature)?;
            codeview
        }
        None => {
            let codeview = CodeView {
                name: String::from("ntkrnlmp.pdb"),
                guid: String::from("ce7ffb00c20b87500211456b3e905c47"),
                age: 1,
            };
            println!(
                "No image path provided, using hardcoded CodeView for Windows 10.0.18362.356:"
            );
            println!("{codeview:#?}");
            codeview
        }
    };

    let entry = cache.entry_from_codeview(codeview)?;
    let profile = entry.profile()?;

    let symbols = Symbols::new(&profile)?;
    let offsets = Offsets::new(&profile)?;

    println!("{symbols:#x?}");
    println!("{offsets:#x?}");

    Ok(())
}
