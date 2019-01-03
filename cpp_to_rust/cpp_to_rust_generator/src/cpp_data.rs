//! Types for handling information about C++ library APIs.

pub use crate::cpp_operator::CppOperator;
use crate::cpp_type::{CppClassType, CppType};
use itertools::Itertools;
use serde_derive::{Deserialize, Serialize};
use std::fmt;

/// One item of a C++ enum declaration
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CppEnumValue {
    /// Identifier
    pub name: String,
    /// Corresponding value
    pub value: u64,
    /// C++ documentation for this item in HTML
    pub doc: Option<String>,
    /// Full type name of the enum this item belongs to
    pub enum_name: CppName,
}

impl CppEnumValue {
    pub fn is_same(&self, other: &CppEnumValue) -> bool {
        self.name == other.name && self.enum_name == other.enum_name && self.value == other.value
    }

    pub fn full_name(&self) -> CppName {
        let mut name = self.enum_name.clone();
        name.parts.pop().expect("enum_name can't be empty");
        name.parts.push(self.name.clone());
        name
    }
}

/// Member field of a C++ class declaration
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct CppClassField {
    /// Identifier
    pub name: String,
    /// Field type
    pub field_type: CppType,
    /// Visibility
    pub visibility: CppVisibility,
    //  /// Size of type in bytes
    //  pub size: Option<usize>,
    /// Name and template arguments of the class type that owns this field
    pub class_type: CppClassType,

    pub is_const: bool,
    pub is_static: bool,
}

impl CppClassField {
    pub fn is_same(&self, other: &CppClassField) -> bool {
        // TODO: when doc is added to CppClassField, ignore it here
        self == other
    }

    pub fn short_text(&self) -> String {
        let visibility_text = match self.visibility {
            CppVisibility::Public => "",
            CppVisibility::Protected => "protected ",
            CppVisibility::Private => "private ",
        };
        format!(
            "class {} {{ {}{} {}; }}",
            self.class_type.to_cpp_pseudo_code(),
            visibility_text,
            self.field_type.to_cpp_pseudo_code(),
            self.name
        )
    }
}

/// Item of base class list in a class declaration
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct CppBaseSpecifier {
    /// Base class type (can include template arguments)
    pub base_class_type: CppClassType,
    /// Index of this base (for classes that have multiple base classes)
    pub base_index: usize,
    /// True if this base is virtual
    pub is_virtual: bool,
    /// Base visibility (public, protected or private)
    pub visibility: CppVisibility,

    /// Name and template arguments of the class type that
    /// inherits this base class
    pub derived_class_type: CppClassType,
}

/// Location of a C++ type's definition in header files.
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct CppOriginLocation {
    // Full path to the include file
    pub include_file_path: String,
    /// Line of the file
    pub line: u32,
    /// Column of the file
    pub column: u32,
}

/// Visibility of a C++ entity. Defaults to `Public`
/// for entities that can't have visibility (like free functions)
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub enum CppVisibility {
    Public,
    Protected,
    Private,
}

/// C++ documentation for a type
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CppTypeDoc {
    /// HTML content
    pub html: String,
    /// Absolute URL to online documentation page for this type
    pub url: String,
    /// Absolute documentation URLs encountered in the content
    pub cross_references: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct CppName {
    /// Parts of the name
    pub parts: Vec<String>,
}

impl CppName {
    pub fn from_one_part(part: impl ToString) -> Self {
        Self {
            parts: vec![part.to_string()],
        }
    }

    pub fn from_parts(parts: &[&str]) -> Self {
        Self {
            parts: parts.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn to_cpp_code(&self) -> String {
        self.parts.join("::")
    }
}

impl fmt::Display for CppName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        for item in self.parts.iter().map(|s| s.as_str()).intersperse("::") {
            write!(f, "{}", item)?;
        }
        Ok(())
    }
}

/// Information about a C++ type declaration
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum CppTypeDataKind {
    Enum,
    Class {
        /// Information about name and template arguments of this type.
        type_base: CppClassType,
    },
}

/// Information about a C++ type declaration
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CppTypeData {
    /// Identifier, including namespaces and nested classes
    /// (separated with "::", like in C++)
    pub name: CppName,
    pub kind: CppTypeDataKind,
    /// C++ documentation for the type
    pub doc: Option<CppTypeDoc>,
    pub is_movable: bool,
}

impl CppTypeData {
    pub fn is_same(&self, other: &CppTypeData) -> bool {
        self.name == other.name && self.kind == other.kind
    }
}

/// Information about a C++ template class
/// instantiation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CppTemplateInstantiation {
    /// Template class name
    pub class_name: CppName,
    /// List of template arguments used in this instantiation
    pub template_arguments: Vec<CppType>,
}

impl CppTypeDataKind {
    /// Checks if the type is a class type.
    pub fn is_class(&self) -> bool {
        match self {
            &CppTypeDataKind::Class { .. } => true,
            _ => false,
        }
    }
    pub fn is_enum(&self) -> bool {
        match self {
            &CppTypeDataKind::Enum { .. } => true,
            _ => false,
        }
    }
}
