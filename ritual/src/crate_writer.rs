use crate::config::CrateDependencySource;
use crate::cpp_code_generator;
use crate::cpp_code_generator::generate_cpp_type_size_requester;
use crate::database::CRATE_DB_FILE_NAME;
use crate::processor::ProcessorData;
use crate::rust_code_generator;
use ritual_common::errors::Result;
use ritual_common::file_utils::{
    copy_file, copy_recursively, crate_version, create_dir, create_dir_all, create_file,
    diff_paths, path_to_str, read_dir, remove_dir_all, repo_dir_path, save_json, save_toml_table,
};
use ritual_common::toml;
use ritual_common::utils::{run_command, MapIfOk};
use ritual_common::BuildScriptData;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Merges `a` and `b` recursively. `b` take precedence over `a`.
fn recursive_merge_toml(a: toml::Value, b: toml::Value) -> toml::Value {
    if a.same_type(&b) {
        if let toml::Value::Array(mut a_array) = a {
            if let toml::Value::Array(mut b_array) = b {
                a_array.append(&mut b_array);
                toml::Value::Array(a_array)
            } else {
                unreachable!()
            }
        } else if let toml::Value::Table(mut a_table) = a {
            if let toml::Value::Table(b_table) = b {
                for (key, value) in b_table {
                    if let Some(old_value) = a_table.remove(&key) {
                        a_table.insert(key, recursive_merge_toml(old_value, value));
                    } else {
                        a_table.insert(key, value);
                    }
                }
                toml::Value::Table(a_table)
            } else {
                unreachable!()
            }
        } else {
            b
        }
    } else {
        b
    }
}

fn toml_table_with_single_item(key: &str, value: impl Into<toml::Value>) -> toml::Value {
    let mut table = toml::value::Table::new();
    table.insert(key.into(), value.into());
    toml::Value::Table(table)
}

/// Generates `Cargo.toml` file and skeleton of the crate.
/// If a crate template was supplied, files from it are
/// copied to the output location.
fn generate_crate_template(data: &mut ProcessorData<'_>, output_path: &Path) -> Result<()> {
    let template_build_rs_path =
        data.config
            .crate_template_path()
            .as_ref()
            .and_then(|crate_template_path| {
                let template_build_rs_path = crate_template_path.join("build.rs");
                if template_build_rs_path.exists() {
                    Some(template_build_rs_path)
                } else {
                    None
                }
            });
    let output_build_rs_path = output_path.join("build.rs");
    if let Some(template_build_rs_path) = &template_build_rs_path {
        copy_file(template_build_rs_path, output_build_rs_path)?;
    } else {
        let mut build_rs_file = create_file(&output_build_rs_path)?;
        write!(
            build_rs_file,
            "{}",
            include_str!("../templates/crate/build.rs")
        )?;
    }

    let mut package = toml::value::Table::new();
    package.insert(
        "name".into(),
        toml::Value::String(data.config.crate_properties().name().into()),
    );
    package.insert(
        "version".into(),
        toml::Value::String(data.config.crate_properties().version().into()),
    );
    package.insert("build".into(), toml::Value::String("build.rs".into()));
    package.insert("edition".into(), toml::Value::String("2018".into()));

    let docs_rs_metadata = toml_table_with_single_item(
        "features",
        vec![toml::Value::String("ritual_rustdoc".into())],
    );
    package.insert(
        "metadata".into(),
        toml_table_with_single_item("docs", toml_table_with_single_item("rs", docs_rs_metadata)),
    );

    let add_dependency = |table: &mut toml::value::Table,
                          name: &str,
                          source: &CrateDependencySource|
     -> Result<()> {
        let (version, local_path) = match source {
            CrateDependencySource::CratesIo { version } => (version.to_string(), None),
            CrateDependencySource::Local { path } => {
                let version = crate_version(path)?;
                (version, Some(path.clone()))
            }
            CrateDependencySource::CurrentWorkspace => {
                let path = data.workspace.crate_path(name);
                let version = data.db.dependency_version(name)?;
                (version.to_string(), Some(path))
            }
        };

        let value = if local_path.is_none() || !data.config.write_dependencies_local_paths() {
            toml::Value::String(version)
        } else {
            let path = diff_paths(&local_path.expect("checked above"), &output_path)?;
            let mut value = toml::value::Table::new();
            value.insert("version".into(), toml::Value::String(version));
            value.insert(
                "path".into(),
                toml::Value::String(path_to_str(&path)?.into()),
            );
            value.into()
        };
        table.insert(name.into(), value);
        Ok(())
    };

    let mut dependencies = toml::value::Table::new();
    if !data
        .config
        .crate_properties()
        .should_remove_default_dependencies()
    {
        add_dependency(
            &mut dependencies,
            "cpp_core",
            &CrateDependencySource::Local {
                path: repo_dir_path("cpp_core")?,
            },
        )?;
    }
    for dep in data.config.crate_properties().dependencies() {
        add_dependency(&mut dependencies, dep.name(), dep.source())?;
    }
    let mut build_dependencies = toml::value::Table::new();
    if !data
        .config
        .crate_properties()
        .should_remove_default_build_dependencies()
    {
        add_dependency(
            &mut build_dependencies,
            "ritual_build",
            &CrateDependencySource::Local {
                path: repo_dir_path("ritual_build")?,
            },
        )?;
    }
    for dep in data.config.crate_properties().build_dependencies() {
        add_dependency(&mut build_dependencies, dep.name(), dep.source())?;
    }
    let mut features = toml::value::Table::new();
    features.insert("ritual_rustdoc".into(), toml::value::Array::new().into());

    let mut table = toml::value::Table::new();
    table.insert("package".into(), package.into());
    table.insert("dependencies".into(), dependencies.into());
    table.insert("build-dependencies".into(), build_dependencies.into());
    table.insert("features".into(), features.into());

    let cargo_toml_data = recursive_merge_toml(
        toml::Value::Table(table),
        toml::Value::Table(data.config.crate_properties().custom_fields().clone()),
    );
    save_toml_table(output_path.join("Cargo.toml"), &cargo_toml_data)?;

    if let Some(template_path) = &data.config.crate_template_path() {
        for item in read_dir(template_path)? {
            let item = item?;
            let target = output_path.join(item.file_name());
            copy_recursively(&item.path(), &target)?;
        }
    }
    if !output_path.join("src").exists() {
        create_dir_all(output_path.join("src"))?;
    }
    Ok(())
}

/// Generates main files and directories of the library.
fn generate_c_lib_template(
    lib_name: &str,
    lib_path: &Path,
    global_header_name: &str,
    include_directives: &[PathBuf],
) -> Result<()> {
    let name_upper = lib_name.to_uppercase();
    let cmakelists_path = lib_path.join("CMakeLists.txt");
    let mut cmakelists_file = create_file(&cmakelists_path)?;

    write!(
        cmakelists_file,
        include_str!("../templates/c_lib/CMakeLists.txt"),
        lib_name_lowercase = lib_name,
        lib_name_uppercase = name_upper
    )?;

    let include_directives_code = include_directives
        .map_if_ok(|d| -> Result<_> { Ok(format!("#include \"{}\"", path_to_str(d)?)) })?
        .join("\n");

    let global_header_path = lib_path.join(&global_header_name);
    let mut global_header_file = create_file(&global_header_path)?;
    write!(
        global_header_file,
        include_str!("../templates/c_lib/global.h"),
        include_directives_code = include_directives_code
    )?;
    Ok(())
}

pub fn run(data: &mut ProcessorData<'_>) -> Result<()> {
    let crate_name = data.config.crate_properties().name();
    let output_path = data.workspace.crate_path(crate_name);

    if output_path.exists() {
        remove_dir_all(&output_path)?;
    }

    create_dir(&output_path)?;
    generate_crate_template(data, &output_path)?;
    data.workspace.update_cargo_toml()?;

    let c_lib_path = output_path.join("c_lib");
    if !c_lib_path.exists() {
        create_dir(&c_lib_path)?;
    }
    let c_lib_name = format!("{}_c", data.config.crate_properties().name());
    let global_header_name = format!("{}_global.h", c_lib_name);
    generate_c_lib_template(
        &c_lib_name,
        &c_lib_path,
        &global_header_name,
        data.config.include_directives(),
    )?;

    cpp_code_generator::generate_cpp_file(
        &data.db,
        &c_lib_path.join("file1.cpp"),
        &global_header_name,
    )?;

    let file = create_file(c_lib_path.join("sized_types.cxx"))?;
    generate_cpp_type_size_requester(data.db, data.config.include_directives(), file)?;

    rust_code_generator::generate(
        &data.db,
        &output_path.join("src"),
        data.config.crate_template_path().map(|s| s.join("src")),
    )?;

    // -p shouldn't be needed, it's a workaround for this bug on Windows:
    // https://github.com/rust-lang/rustfmt/issues/2694
    run_command(
        Command::new("cargo")
            .arg("fmt")
            .arg(format!("-p{}", crate_name))
            .current_dir(&output_path),
    )?;
    run_command(
        Command::new("rustfmt")
            .arg("src/ffi.in.rs")
            .current_dir(&output_path),
    )?;

    save_json(
        output_path.join("build_script_data.json"),
        &BuildScriptData {
            cpp_build_config: data.config.cpp_build_config().clone(),
            cpp_wrapper_lib_name: c_lib_name,
            known_targets: data.db.environments().to_vec(),
        },
        None,
    )?;

    copy_file(
        data.workspace.database_path(crate_name),
        output_path.join(CRATE_DB_FILE_NAME),
    )?;

    Ok(())
}
