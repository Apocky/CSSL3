//! Calling-convention ABI + object-file format enumerations.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND § ABI line + `specs/14_BACKEND.csl` § ABI.

use core::fmt;

/// Calling-convention ABI for generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Abi {
    /// System V AMD64 (Linux + Mac-Intel + BSD).
    SysVAmd64,
    /// Microsoft Windows-x64 (x64 calling convention).
    WindowsX64,
    /// Darwin-AMD64 (macOS-Intel ; uses SysV with Apple-extensions).
    DarwinAmd64,
}

impl Abi {
    /// Short-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SysVAmd64 => "sysv",
            Self::WindowsX64 => "win64",
            Self::DarwinAmd64 => "darwin",
        }
    }

    /// Typical object-format paired with this ABI.
    #[must_use]
    pub const fn typical_object_format(self) -> ObjectFormat {
        match self {
            Self::SysVAmd64 => ObjectFormat::Elf,
            Self::WindowsX64 => ObjectFormat::Coff,
            Self::DarwinAmd64 => ObjectFormat::MachO,
        }
    }
}

impl fmt::Display for Abi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Object-file format for the emitted artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectFormat {
    /// ELF (Linux + BSD).
    Elf,
    /// COFF / PE (Windows).
    Coff,
    /// Mach-O (macOS + iOS).
    MachO,
}

impl ObjectFormat {
    /// Short-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Elf => "elf",
            Self::Coff => "coff",
            Self::MachO => "macho",
        }
    }

    /// Canonical object-file extension (e.g. `".o"` for ELF, `".obj"` for COFF).
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Elf => ".o",
            Self::Coff => ".obj",
            Self::MachO => ".o",
        }
    }
}

impl fmt::Display for ObjectFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{Abi, ObjectFormat};

    #[test]
    fn abi_names() {
        assert_eq!(Abi::SysVAmd64.as_str(), "sysv");
        assert_eq!(Abi::WindowsX64.as_str(), "win64");
        assert_eq!(Abi::DarwinAmd64.as_str(), "darwin");
    }

    #[test]
    fn object_format_names() {
        assert_eq!(ObjectFormat::Elf.as_str(), "elf");
        assert_eq!(ObjectFormat::Coff.as_str(), "coff");
        assert_eq!(ObjectFormat::MachO.as_str(), "macho");
    }

    #[test]
    fn object_format_extensions() {
        assert_eq!(ObjectFormat::Elf.extension(), ".o");
        assert_eq!(ObjectFormat::Coff.extension(), ".obj");
        assert_eq!(ObjectFormat::MachO.extension(), ".o");
    }

    #[test]
    fn abi_typical_object_pairing() {
        assert_eq!(Abi::SysVAmd64.typical_object_format(), ObjectFormat::Elf);
        assert_eq!(Abi::WindowsX64.typical_object_format(), ObjectFormat::Coff);
        assert_eq!(
            Abi::DarwinAmd64.typical_object_format(),
            ObjectFormat::MachO
        );
    }
}
