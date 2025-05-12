
//////
//
// Language config
//

// No point enabling internal features if we still get warnings
#![allow(internal_features)]

// Enable intrinsics so we can debug this build script
#![feature(core_intrinsics)]



//////
//
// Imports
//

// Standard library
use std::{env, path::{Path, PathBuf}, fs, process};
use std::fmt::Display;
// CMake crate
use cmake;



//////
//
// Errors
//

/// A simple error indicating that an external command invoked via [`std::process::Command`] failed.
#[derive(Debug)]
pub struct CommandFailedError {
	/// A short descriptive name for the command that failed.
	pub command_name: String,
}
impl Display for CommandFailedError {
	fn fmt (&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(formatter, "CommandFailedError[`{}`]", self.command_name)
	}
}
impl std::error::Error for CommandFailedError {}



//////
//
// Functions
//

/// Find the path to the target directory of the current Cargo invocation.
/// Adapted from the following issue: https://github.com/rust-lang/cargo/issues/9661#issuecomment-1722358176
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

/// Check the given [std::process::Output](process output) for errors, emitting *Cargo* output detailing the problem if
/// the output does not indicate success.
fn check_process_output (output: std::process::Output, command_name: impl AsRef<str>) -> Result<(), CommandFailedError>
{
	if !output.status.success()
	{
		println!("cargo::warning={} {}", command_name.as_ref(), output.status);
		for line in String::from_utf8_lossy(&output.stderr).lines() {
			println!("cargo::warning={} stdout: {line}", command_name.as_ref());
		}
		for line in String::from_utf8_lossy(&output.stdout).lines() {
			println!("cargo::warning={} stderr: {line}", command_name.as_ref());
		}
		println!("cargo::error={} failed", command_name.as_ref());
		Err(CommandFailedError{ command_name: String::from(command_name.as_ref()) })
	}
	else {
		Ok(())
	}
}

/// A convenience shorthand for calling [`check_process_output()`] with the `CMake` as the *command_name*.
fn check_cmake_output (output: std::process::Output) -> Result<(), CommandFailedError> {
	check_process_output(output, "CMake")
}

/// Recursively copy an entire directory tree.
fn copy_recursively<SrcPathRef: AsRef<Path>, DstPathRef: AsRef<Path>> (source: SrcPathRef, dest: DstPathRef)
	-> Result<(), Box<dyn std::error::Error>>
{
	fs::create_dir_all(&dest)?;
	for entry in fs::read_dir(source)?
	{
		let entry = entry?;
		let filetype = entry.file_type()?;
		if filetype.is_dir() {
			copy_recursively(entry.path(), dest.as_ref().join(entry.file_name()))?;
		} else {
			fs::copy(entry.path(), dest.as_ref().join(entry.file_name()))?;
		}
	}
	Ok(())
}

/// Custom build steps – build Slang SDK and handle all additional steps required to make it work on WASM.
fn main () -> Result<(), Box<dyn std::error::Error>>
{
	////
	// Preamble

	// Launch VS Code LLDB debugger if it is installed and attach to the build script
	let url = format!(
		"vscode://vadimcn.vscode-lldb/launch/config?{{'request':'attach','pid':{}}}", std::process::id()
	);
	if let Ok(result) = std::process::Command::new("code").arg("--open-url").arg(url).output()
	    && result.status.success() {
		std::thread::sleep(std::time::Duration::from_secs(4)); // <- give debugger time to attach
	}

	// Obtain the output directory
	let out_dir = env::var("OUT_DIR")
		.map(PathBuf::from)
		.expect("The output directory must be set by Cargo as an environment variable");

	// Obtain the target directory
	let target_dir = get_cargo_target_dir(out_dir.as_path())
		.expect("The Cargo target directory should be inferrable from OUT_DIR");


	////
	// Configure and build Slang

	// Determine CMake install destination and build type
	let (cmake_build_type, cmake_install_dest) = if cfg!(debug_assertions) {
		("Debug", out_dir.join("slang-install"))
	} else {
		("Release", out_dir.join("slang-install"))
	};

	// Obtain Slang source path
	let slang_path = fs::canonicalize("../vendor/slang")
		.expect("Slang repository must be included as a submodule inside the '/vendor' directory");
	let slang_lib_type;
	match env::var("CARGO_CFG_TARGET_ARCH").expect("Unable to determine target architecture").as_ref()
	{
		// WASM is not yet supported
		"wasm32" => {
			// cmake --workflow --preset generators --fresh
			let generators_build_path =  slang_path.join("build");
			let generators_build_path_arg = generators_build_path.to_str().expect(
				"Slang generators build directory must have String-representable name"
			).to_owned();
			let cmake_result = process::Command::new("cmake")
				.current_dir(slang_path.as_path())
				.args(["--workflow", "--preset", "generators", "--fresh"])
				.output()
				.expect("Could not spawn CMake process");
			check_cmake_output(cmake_result)?;

			// cmake --install build --prefix generators --component generators
			let generators_dir =  out_dir.join("slang-generators");
			if !generators_dir.exists() {
				fs::create_dir(generators_dir.as_path()).expect("Failed to create generators directory");
			}
			let generators_dir_arg = generators_dir.to_str().expect(
				"Slang generators build directory must have String-representable name"
			).to_owned();
			let cmake_result = process::Command::new("cmake")
				.current_dir(slang_path.as_path())
				.args([
					"--install", generators_build_path_arg.as_str(), "--prefix", generators_dir_arg.as_str(),
					"--component", "generators"
				])
				.output()
				.expect("Could not spawn CMake process");
			check_cmake_output(cmake_result)?;

			// emcmake cmake -DSLANG_GENERATORS_PATH=generators/bin --preset emscripten -G "Ninja"
			let generators_dir_option = format!(
				"-DSLANG_GENERATORS_PATH={}",
				generators_dir.join("bin").to_str()
					.unwrap() // <- this can't fail because string-representability already verified for generators_dir
			);
			let slang_build_dir =  out_dir.join("slang-build");
			if !slang_build_dir.exists() {
				fs::create_dir(slang_build_dir.as_path()).expect("Failed to create Slang build directory");
			}
			let slang_build_dir_arg = slang_build_dir.to_str().expect(
				"Slang build directory must have String-representable name"
			).to_owned();
			let cmake_result = process::Command::new("emcmake")
				.current_dir(slang_path.as_path())
				.args([
					"cmake", generators_dir_option.as_str(), "--preset", "emscripten", "-G", "Ninja",
					"-B", slang_build_dir_arg.as_str()
				])
				.output()
				.expect("Could not spawn emcmake process");
			check_process_output(cmake_result, "emcmake")?;

			// cmake --build --preset emscripten --target slang-wasm
			let cmake_result = process::Command::new("cmake")
				.current_dir(slang_build_dir.as_path())
				.args(["--build", ".", "--target", "slang-wasm"])
				.output()
				.expect("Could not spawn CMake process");
			check_cmake_output(cmake_result)?;

			// Perform manual Slang WASM install
			if !cmake_install_dest.exists() {
				fs::create_dir(cmake_install_dest.as_path()).expect("Failed to create Slang install directory");
			}
			let slang_wasm_release_artifacts_dir = slang_build_dir.join("Release");
			if !slang_wasm_release_artifacts_dir.exists() {
				println!("cargo::error={}", "WASM build did not result in release artifacts in expected place");
				println!("cargo::error=Expected place: {}", slang_wasm_release_artifacts_dir.display());
				return Ok(());
			}
			copy_recursively(slang_wasm_release_artifacts_dir, cmake_install_dest.as_path())?;

			// Copy Slang WASM modules to target dir if requested
			if env::var("CARGO_FEATURE_COPY_LIBS").is_ok()
			{
				// Copy libs
				for entry in fs::read_dir(cmake_install_dest.join("bin"))
					.expect(
						"The Slang repository clone should have received a 'build.em/Release/bin' subdirectory"
				){
					let entry = entry.unwrap();
					if entry.file_type().unwrap().is_file() {
						fs::copy(entry.path(), target_dir.join(entry.file_name()))
							.expect(format!(
								"Failed to copy '{}' to '{}'", entry.path().display(), target_dir.display()
							).as_str());
					}
				};
			}
			slang_lib_type = "staticlib";
		},

		// Native Slang build
		_ => {
			// Build and install into OUT_DIR

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
			slang_lib_type = "dylib";
		}
	}


	////
	// Generate bindings

	// Check prerequisites
	let include_file;
	let include_path;
	let include_path_arg;
	let slang_dir = {
		include_file = cmake_install_dest.join("include/slang.h");
		include_path = cmake_install_dest.join("include");
		include_path_arg = format!("-I{}", include_path.display());
		fs::canonicalize(cmake_install_dest.as_path()).expect(
			format!("Slang SDK should have been successfully build in '{}'", cmake_install_dest.display()).as_str()
		)
	};

	link_libraries(&slang_dir, slang_lib_type);

	bindgen::builder()
		.header(slang_dir.join(include_file).to_str().unwrap())
		.clang_arg("-v")
		.clang_arg("-xc++")
		.clang_arg("-std=c++17")
		.clang_arg(include_path_arg)
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
		.write_to_file(out_dir.join("bindings.rs"))?;
	Ok(())
}

fn link_libraries(slang_dir: &Path, slang_lib_type: &str) {
	let lib_dir = slang_dir.join("lib");

	if !lib_dir.is_dir() {
		panic!("Couldn't find the `lib` subdirectory in the Slang installation directory.")
	}

	println!("cargo:rustc-link-search=native={}", lib_dir.display());
	println!("cargo:rustc-link-lib={slang_lib_type}=slang");
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
