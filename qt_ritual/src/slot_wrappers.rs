use crate::detect_signal_argument_types::detect_signal_argument_types;
use itertools::Itertools;
use log::trace;
use ritual::cpp_ffi_data::{CppFfiItem, QtSlotWrapper};
use ritual::cpp_ffi_generator::{ffi_type, FfiNameProvider};
use ritual::cpp_type::{CppFunctionPointerType, CppPointerLikeTypeKind, CppType, CppTypeRole};
use ritual::processor::ProcessorData;
use ritual_common::errors::Result;
use ritual_common::utils::MapIfOk;
use std::iter::once;

/// Generates slot wrappers for all encountered argument types
/// (excluding types already handled in the dependencies).
fn generate_slot_wrapper(
    arguments: &[CppType],
    name_provider: &mut FfiNameProvider,
) -> Result<QtSlotWrapper> {
    let ffi_types = arguments.map_if_ok(|t| ffi_type(&t, CppTypeRole::NotReturnType))?;
    let class_path = name_provider.create_path(&format!(
        "slot_wrapper_{}",
        arguments.iter().map(CppType::ascii_caption).join("_")
    ));

    let void_ptr = CppType::PointerLike {
        is_const: false,
        kind: CppPointerLikeTypeKind::Pointer,
        target: Box::new(CppType::Void),
    };
    let func_arguments = once(void_ptr.clone())
        .chain(ffi_types.iter().map(|t| t.ffi_type().clone()))
        .collect();

    let function_type = CppFunctionPointerType {
        return_type: Box::new(CppType::Void),
        arguments: func_arguments,
        allows_variadic_arguments: false,
    };

    let qt_slot_wrapper = QtSlotWrapper {
        signal_arguments: arguments.to_vec(),
        class_path: class_path.clone(),
        arguments: ffi_types,
        function_type: function_type.clone(),
    };
    Ok(qt_slot_wrapper)
}

pub fn add_slot_wrappers(data: &mut ProcessorData<'_>) -> Result<()> {
    let all_types = detect_signal_argument_types(data)?;

    let mut name_provider = FfiNameProvider::new(data);

    for arg_types in all_types {
        let arg_types_text = arg_types.iter().map(CppType::to_cpp_pseudo_code).join(", ");

        let found = data
            .db
            .all_ffi_items()
            .filter_map(|item| item.item.as_slot_wrapper_ref())
            .any(|item| item.signal_arguments == arg_types);
        if found {
            trace!("slot wrapper already exists: {}", arg_types_text);
        } else {
            match generate_slot_wrapper(&arg_types, &mut name_provider) {
                Ok(slot_wrapper) => {
                    let id = data
                        .db
                        .add_ffi_item(None, CppFfiItem::QtSlotWrapper(slot_wrapper))?;
                    if id.is_some() {
                        trace!("adding slot wrapper for args: ({})", arg_types_text);
                    }
                }
                Err(err) => {
                    trace!(
                        "failed to add slot wrapper for args: ({}): {}",
                        arg_types_text,
                        err
                    );
                }
            }
        }
    }
    Ok(())
}
