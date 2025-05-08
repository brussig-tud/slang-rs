
//////
//
// Imports
//

// Standard library
use std::{env, path::{Path, PathBuf}, fs};

// CMake crate
use cmake;



//////
//
// Functions
//

/// Find the path to the target directory of the current Cargo invokation
// Adapted from the following issue: https://github.com/rust-lang/cargo/issues/9661#issuecomment-1722358176
fn get_cargo_target_dir(out_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>>
{
	let profile = env::var("PROFILE")?;
	let mut target_dir = None;
	let mut sub_path = out_dir;
	while let Some(parent) = sub_path.parent() {
		if parent.ends_with(&profile) {
			target_dir = Some(parent);
			break;
		}
		sub_path = parent;
	}
	let target_dir = target_dir.ok_or("<not_found>")?;
	Ok(target_dir.to_path_buf())
}

/// Custom build steps – build Slang SDK and handle all additional steps required to make it work on WASM.
fn main ()
{
	// Obtain the output directory
	let out_dir = env::var("OUT_DIR")
		.map(PathBuf::from)
		.expect("The output directory must be set by Cargo as an environment variable");

	// Obtain the target directory
	let target_dir = get_cargo_target_dir(out_dir.as_path())
		.expect("The Cargo target directory should be inferrable from OUT_DIR");

	// Determine CMake install destination and build type
	let (cmake_build_type, cmake_install_dest) = if cfg!(debug_assertions) {
		("Debug", out_dir.join("slang-install"))
	} else {
		("Release", out_dir.join("slang-install"))
	};

	// Configure and build Slang
	match env::var("CARGO_CFG_TARGET_ARCH").expect("Unable to determine target architecture").as_ref()
	{
		// WASM is not yet supported
		"wasm32" => {
			println!("cargo::error={}", "WASM builds not yet supported");
			return;
		},

		// Native Slang build
		_ => {
			// Build and install into OUT_DIR
			let slang_path = fs::canonicalize("../vendor/slang")
				.expect("Slang repository must be included as a submodule inside the '/vendor' directory");
			let _dst = cmake::Config::new(slang_path)
				.profile(cmake_build_type)
				.define("CMAKE_INSTALL_PREFIX", cmake_install_dest.as_os_str())
				.build();

			// Copy libs to target dir if requested
			if env::var("CARGO_FEATURE_COPY_LIBS").is_ok()
			{
				// Copy libs
				for entry in fs::read_dir(cmake_install_dest.join("lib"))
					.expect("The Slang installation directory must contain a 'lib' subdirectory")
				{
					let entry = entry.unwrap();
					if entry.file_type().unwrap().is_file() {
						fs::copy(entry.path(), target_dir.join(entry.file_name()))
							.expect(format!(
								"Failed to copy '{}' to '{}'", entry.path().display(), target_dir.display()
							).as_str());
					}
				};

				// Set linker flags accordingly
				if !env::var("CARGO_CFG_WINDOWS").is_ok() {
					let link_args = "-Wl,-rpath=$ORIGIN";
					println!("cargo:rustc-link-arg={link_args}");
					println!("cargo:REQUIRED_LINK_ARGS={link_args}");
				}
			}
		}
	}

	let include_file;
	let slang_dir = {
		include_file = cmake_install_dest.join("include/slang.h");
		fs::canonicalize(cmake_install_dest.as_path()).expect(
			format!("Slang SDK was successfully build in '{}'", cmake_install_dest.display()).as_str()
		)
	};

	link_libraries(&slang_dir);

	bindgen::builder()
		.header(slang_dir.join(include_file).to_str().unwrap())
		.clang_arg("-v")
		.clang_arg("-xc++")
		.clang_arg("-std=c++17")
		.allowlist_function("spReflection.*")
		.allowlist_function("spComputeStringHash")
		.allowlist_function("slang_.*")
		.allowlist_type("slang.*")
		.allowlist_var("SLANG_.*")
		.with_codegen_config(
			bindgen::CodegenConfig::FUNCTIONS
				| bindgen::CodegenConfig::TYPES
				| bindgen::CodegenConfig::VARS,
		)
		.parse_callbacks(Box::new(ParseCallback {}))
		.default_enum_style(bindgen::EnumVariation::Rust {
			non_exhaustive: false,
		})
		.constified_enum("SlangProfileID")
		.constified_enum("SlangCapabilityID")
		.vtable_generation(true)
		.layout_tests(false)
		.derive_copy(true)
		.generate()
		.expect("Couldn't generate bindings.")
		.write_to_file(out_dir.join("bindings.rs"))
		.expect("Couldn't write bindings.");
}

fn link_libraries(slang_dir: &Path) {
	let lib_dir = slang_dir.join("lib");

	if !lib_dir.is_dir() {
		panic!("Couldn't find the `lib` subdirectory in the Slang installation directory.")
	}

	println!("cargo:rustc-link-search=native={}", lib_dir.display());
	println!("cargo:rustc-link-lib=dylib=slang");
}

#[derive(Debug)]
struct ParseCallback {}

impl bindgen::callbacks::ParseCallbacks for ParseCallback {
	fn enum_variant_name(
		&self,
		enum_name: Option<&str>,
		original_variant_name: &str,
		_variant_value: bindgen::callbacks::EnumVariantValue,
	) -> Option<String> {
		let enum_name = enum_name?;

		// Map enum names to the part of their variant names that needs to be trimmed.
		// When an enum name is not in this map the code below will try to trim the enum name itself.
		let mut map = std::collections::HashMap::new();
		map.insert("SlangMatrixLayoutMode", "SlangMatrixLayout");
		map.insert("SlangCompileTarget", "Slang");

		let trim = map.get(enum_name).unwrap_or(&enum_name);
		let new_variant_name = pascal_case_from_snake_case(original_variant_name);
		let new_variant_name = new_variant_name.trim_start_matches(trim);
		Some(new_variant_name.to_string())
	}
}

/// Converts `snake_case` or `SNAKE_CASE` to `PascalCase`.
/// If the input is already in `PascalCase` it will be returned as is.
fn pascal_case_from_snake_case(snake_case: &str) -> String {
	let mut result = String::new();

	let should_lower = snake_case
		.chars()
		.filter(|c| c.is_alphabetic())
		.all(|c| c.is_uppercase());

	for part in snake_case.split('_') {
		for (i, c) in part.chars().enumerate() {
			if i == 0 {
				result.push(c.to_ascii_uppercase());
			} else if should_lower {
				result.push(c.to_ascii_lowercase());
			} else {
				result.push(c);
			}
		}
	}

	result
}
