
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
use std::{
	env, fs, process, fmt::Display, path::{Path, PathBuf}, time::{Duration, SystemTime}, ops::Sub, sync::LazyLock
};

// Filetime crate
use fs_set_times::*;

// Reqwest crate
use reqwest;

// Zip-extract crate
use zip_extract as zip;



//////
//
// Constants
//

/// The *Slang* version this crate is tested against.
const SLANG_VERSION: &str = "2025.10.3";

/// Evaluates to the pattern according to which the parent URL for *Slang* binary releases is composed.
#[allow(non_snake_case)]
macro_rules! SLANG_RELEASE_URL_BASE {() => {"https://github.com/shader-slang/slang/releases/download/v{version}/"};}

/// Evaluates to the pattern according to which *Slang* binary releases are named.
#[allow(non_snake_case)]
macro_rules! SLANG_PACKAGE_NAME {() => {"slang-{version}-{os}-{arch}.zip"};}

/// Global storing the `SystemTime` when the build script main functions gained control of execution flow.
static SCRIPT_START_TIME: LazyLock<SystemTime> = LazyLock::new(|| SystemTime::now().sub(Duration::from_secs(5)));



//////
//
// Errors
//

/// An error indicating that an external command invoked via [`std::process::Command`] failed, holding the complete
/// [output](std::process::Output) that the command produced.
#[derive(Debug)]
pub struct CommandFailedError {
	/// A short descriptive name for the command that failed.
	pub command_name: String,
	pub output: std::process::Output
}
impl CommandFailedError
{
	pub fn format_stdstream (formatter: &mut std::fmt::Formatter<'_>, prefix: &str, stream_buf: &[u8])
	-> std::fmt::Result {
		for line in String::from_utf8_lossy(stream_buf).lines() {
			write!(formatter, "{prefix}{line}")?;
		}
		Ok(())
	}
}
impl Display for CommandFailedError {
	fn fmt (&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(formatter, "CommandFailedError[`{}` -> {}]", self.command_name, self.output.status)?;
		Self::format_stdstream(formatter, " stdout: ", &self.output.stdout)?;
		Self::format_stdstream(formatter, " stderr: ", &self.output.stderr)
	}
}
impl std::error::Error for CommandFailedError {}

/// A simple error indicating that some entity could not be represented exactly as a Unicode string, e.g. because it
/// contains non-displayable characters.
#[derive(Debug)]
pub struct NotStringRepresentableError {
	/// A lossy representation of the problematic entity.
	pub lossy_string: String,
}
impl Display for NotStringRepresentableError {
	fn fmt (&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(formatter, "NotStringRepresentableError[`{}`]", self.lossy_string)
	}
}
impl std::error::Error for NotStringRepresentableError {}

/// A simple error indicating that a web request did not result in a `200 OK` response.
#[derive(Debug)]
pub struct HttpResponseNotOkError {
	/// The URL of the request that did not respond with `200 OK`.
	pub url: String,

	/// The full response of the request that did not respond with `200 OK`.
	pub response: reqwest::blocking::Response
}
impl HttpResponseNotOkError {
	/// Create a new instance for the given `url` and `response`.o
	pub fn new (url: impl Into<String>, response: reqwest::blocking::Response) -> Self { Self {
		url: url.into(), response
	}}
}
impl Display for HttpResponseNotOkError {
	fn fmt (&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(formatter, "HttpResponseNotOkError[`{}`<-{}]", self.response.status(), self.url)
	}
}
impl std::error::Error for HttpResponseNotOkError {}



//////
//
// Structs
//

/// Stores information about a *Slang* installation.
struct SlangInstall {
	directory: PathBuf,

	#[allow(dead_code)] // we might need this in the future
	include_path: PathBuf,

	include_file: PathBuf,
	include_path_arg: String,
	lib_type: &'static str
}



//////
//
// Functions
//

/// Converts the given path to a unicode string slice if possible, erroring out if the path contains non-displayable
/// characters.
fn path_to_str<'path, PathRef: AsRef<Path>+'path+?Sized> (path: &'path PathRef)
-> Result<&'path str, NotStringRepresentableError> {
	path.as_ref().to_str().ok_or(NotStringRepresentableError {
		lossy_string: path.as_ref().to_string_lossy().to_string()
	})
}

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

///
pub fn set_timestamp (path: impl AsRef<Path>, timepoint: SystemTime) -> Result<(), Box<dyn std::error::Error>>
{
	if path.as_ref().is_dir() {
		Ok(set_mtime(path, SystemTimeSpec::from(timepoint))?)
	}
	else {
		let file = fs::File::options().write(true).open(path)?;
		Ok(file.set_times(fs::FileTimes::new().set_modified(timepoint))?)
	}
}

///
pub fn set_timestamp_with_warning (path: impl AsRef<Path>, timepoint: SystemTime) -> bool
{
	if let Err(err) = set_timestamp(path.as_ref(), timepoint) {
		println!(
			"cargo::warning=set_timestamp_with_warning: Failed to set timestamp for '{}': {}",
			path.as_ref().display(), err
		);
		false
	}
	else { true }
}

///
pub fn set_timestamp_recursively (path: impl AsRef<Path>, timepoint: SystemTime)
	-> Result<bool, Box<dyn std::error::Error>>
{
	let mut no_problem = true;
	for entry in fs::read_dir(path.as_ref())?
	{
		let entry = entry?;
		let filetype = entry.file_type()?;
		if filetype.is_dir() {
			no_problem = set_timestamp_recursively(entry.path(), timepoint)? && no_problem;
		} else {
			no_problem = set_timestamp_with_warning(entry.path(), timepoint) && no_problem;
		}
	}
	no_problem = set_timestamp_with_warning(path, timepoint) && no_problem;
	Ok(no_problem)
}

/// Request from the given URL and return the full response body as a sequence of bytes.
pub fn download (url: impl reqwest::IntoUrl) -> Result<bytes::Bytes, Box<dyn std::error::Error>> {
	let dl_response = reqwest::blocking::get(url.as_str())?;
	if dl_response.status() != reqwest::StatusCode::OK {
		return Err(HttpResponseNotOkError::new(url.as_str(), dl_response).into())
	}
	Ok(dl_response.bytes()?)
}

/// Request from the given URL and store the response body in the given file.
pub fn download_to_file (url: impl reqwest::IntoUrl, filepath: impl AsRef<crate::Path>)
-> Result<(), Box<dyn std::error::Error>> {
	let response_bytes = download(url)?;
	Ok(fs::write(filepath.as_ref(), response_bytes)?)
}

/// Request an archive file from the given URL and extract its contents (without the root/parent directory if the
/// archive contains one) to the given path.
pub fn download_and_extract (url: impl reqwest::IntoUrl, dirpath: impl AsRef<crate::Path>)
	-> Result<(), Box<dyn std::error::Error>>
{
	let response_bytes = download(url).or_else(|err| {
		println!("cargo::error=download_and_extract: Failed to download archive!");
		println!("cargo::error=download_and_extract: download error: {}", err.as_ref());
		Err(err)
	})?;
	Ok(zip::extract(std::io::Cursor::new(response_bytes), dirpath.as_ref(), true)?)
}

///
pub fn depend_on_downloaded_file (url: impl reqwest::IntoUrl, filepath: impl AsRef<crate::Path>)
-> Result<(), Box<dyn std::error::Error>> {
	println!("cargo::rerun-if-changed={}", filepath.as_ref().display());
	download_to_file(url, filepath.as_ref())?;
	if !set_timestamp_with_warning(filepath.as_ref(), *SCRIPT_START_TIME) {
		println!("cargo::warning=depend_on_downloaded_file: Problem setting time stamp – \
		          Cargo change detection could fail")
	}
	Ok(())
}

///
pub fn depend_on_extracted_directory (archive_path: impl AsRef<crate::Path>, dirpath: impl AsRef<crate::Path>)
-> Result<(), Box<dyn std::error::Error>> {
	println!("cargo::rerun-if-changed={}", dirpath.as_ref().display());
	zip::extract(std::fs::File::open(archive_path.as_ref())?, dirpath.as_ref(), true)?;
	if !set_timestamp_recursively(dirpath.as_ref(), *SCRIPT_START_TIME)? {
		println!("cargo::warning=depend_on_extracted_directory: Problem setting time stamps – \
		          Cargo change detection could fail")
	}
	Ok(())
}

///
pub fn depend_on_downloaded_directory (url: impl reqwest::IntoUrl, dirpath: impl AsRef<crate::Path>)
-> Result<(), Box<dyn std::error::Error>> {
	println!("cargo::rerun-if-changed={}", dirpath.as_ref().display());
	download_and_extract(url, dirpath.as_ref())?;
	if !set_timestamp_recursively(dirpath.as_ref(), *SCRIPT_START_TIME)? {
		println!("cargo::warning=depend_on_downloaded_directory: Problem setting time stamps – \
		          Cargo change detection could fail")
	}
	Ok(())
}

/// Check the given [std::process::Output](process output) for errors, emitting *Cargo* output detailing the problem if
/// the output does not indicate success.
fn check_process_output (output: std::process::Output, command_name: impl AsRef<str>) -> Result<(), CommandFailedError>
{
	if !output.status.success() {
		Err(CommandFailedError{ command_name: String::from(command_name.as_ref()), output })
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

/// Builds *Slang*-native from the given source directory with the given *CMake* generator and build type, and installs
/// it to the specified target directory if the build was successful.
fn build_slang_native_with_generator (src_dir: &Path, install_target_dir: &Path, cmake_generator: &str, cmake_build_type: &str)
	-> Result<(), Box<dyn std::error::Error>>
{
	// Preamble
	let install_target_dir_str = path_to_str(install_target_dir)?;
	let install_dir_arg = format!("-DCMAKE_INSTALL_PREFIX={install_target_dir_str}");
	let build_type_arg = format!("-DCMAKE_BUILD_TYPE={cmake_build_type}");

	// Configure and generate
	let cmake_result = process::Command::new("cmake")
		.current_dir(src_dir)
		.args([
			"--preset", "default", install_dir_arg.as_str(), build_type_arg.as_str(),
			"-DCMAKE_CONFIGURATION_TYPES=Debug;Release", "-G", cmake_generator
		])
		.output()
		.expect("build_slang_native: Could not spawn CMake configure/generate process");
	check_cmake_output(cmake_result)?;

	// Build
	let cmake_result = process::Command::new("cmake")
		.current_dir(src_dir)
		.args(["--build", "--preset", cmake_build_type.to_lowercase().as_str()])
		.output()
		.expect("build_slang_native: Could not spawn CMake build process");
	check_cmake_output(cmake_result)?;

	// Install
	let cmake_result = process::Command::new("cmake")
		.current_dir(src_dir)
		.args([
			"--install", "build", "--prefix", install_target_dir_str, "--config", cmake_build_type
		])
		.output()
		.expect("build_slang_native: Could not spawn CMake install process");
	Ok(check_cmake_output(cmake_result)?)
}

/// Try to build *Slang*-native for the current build type, trying a several generators that make sense for the current
/// platform until one of them succeeds, and install it into the indicated target directory. If none of the generators
/// work, the function returns an error containing the output of the final *CMake* invocation that was run.
fn try_build_slang_native (src_dir: &Path, install_target_dir: &Path)
	-> Result<(), Box<dyn std::error::Error>>
{
	// Infer CMake build type from the current *Cargo* profile
	let cmake_build_type = match std::env::var("PROFILE")?.as_str() {
		"debug"   => "Debug",
		"release" => "Release",
		profile => {
			println!("cargo::error=try_build_slang_native: Unknown Cargo profile: {profile}");
			return Err(format!("Unknown Cargo profile: {profile}").into());
		}
	};

	// Infer generators to try
	let generators;
	#[cfg(target_os="windows")] {
		generators = ["Ninja", "Visual Studio 17 2022"];
	}
	#[cfg(target_os="macos")] {
		 generators = ["Ninja", "Xcode"];
	}
	#[cfg(all(not(target_os="windows"),not(target_os="macos")))] {
		generators = ["Ninja", "Unix Makefiles"];
	}

	// Try building with the generator
	let mut result = Ok(());
	for generator in generators {
		result = build_slang_native_with_generator(src_dir, install_target_dir, generator, cmake_build_type);
		if result.is_ok() {
			return result;
		}
		std::fs::remove_dir_all(src_dir.join("build"))
			.expect("Failed to clean up the Slang build directory after failed CMake build attempt");
		continue;
	}
	result
}

///
fn get_slang_install_at_path (slang_install_path: impl AsRef<Path>, lib_type: &'static str) -> Option<SlangInstall>
{
	// Validate directory structure
	let include_file = if let Ok(existing_file) = fs::canonicalize(
		slang_install_path.as_ref().join("include/slang.h")
	){
		existing_file
	} else {
		return None;
	};
	let include_path = include_file.parent().unwrap().to_owned();
	let directory = include_path.parent().unwrap().to_owned();

	// Return the installation
	Some(SlangInstall {
		directory,
		include_path_arg: format!("-I{}", include_path.display()),
		include_path, include_file, lib_type
	})
}

///
fn use_slang_from_system () -> Result<Option<SlangInstall>, Box<dyn std::error::Error>>
{
	// Depend on the relevant environment variables
	println!("cargo::rerun-if-env-changed=SLANG_DIR");
	//println!("cargo::rerun-if-env-changed=VULKAN_SDK"); // TODO: support getting Slang from Vulkan SDK

	// Try to find an installation
	if let Ok(slang_dir) = env::var("SLANG_DIR").map(PathBuf::from) {
		Ok(get_slang_install_at_path(slang_dir, "dylib"))
	}
	else {
		Ok(None)
	}
}

///
fn use_downloaded_slang (out_dir: &Path) -> Result<Option<SlangInstall>, Box<dyn std::error::Error>>
{
	// Determine architecture
	let architecture =
		     if cfg!(target_arch="x86_64") { "x86_64" }
		else if cfg!(target_arch="aarch64") { "aarch64" }
		else { return Err("Unsupported build architecture".into()); };

	// Determine operating system
	let operating_system =
		     if cfg!(target_os="linux") { "linux" }
		else if cfg!(target_os="windows") { "windows" }
		else if cfg!(target_os="macos") { "macos" }
		else { return Err("Unsupported build operating system".into()); };

	// Compile package name and URL
	let package_name = format!(
		SLANG_PACKAGE_NAME!(), version=SLANG_VERSION, os=operating_system, arch=architecture
	);
	let package_url = reqwest::Url::parse(
		format!(SLANG_RELEASE_URL_BASE!(), version=SLANG_VERSION).as_str()
	)?.join(package_name.as_str())?;

	// Attempt the download
	let archive_filepath = out_dir.join(package_name);
	let slang_dir = out_dir.join("slang-install");
	download_to_file(package_url, archive_filepath.as_path())?;
	if depend_on_extracted_directory(archive_filepath, slang_dir.as_path()).is_ok() {
		Ok(get_slang_install_at_path(slang_dir, "dylib"))
	}
	else {
		Ok(None)
	}
}

///
fn use_internally_built_slang (out_dir: &Path) -> Result<Option<SlangInstall>, Box<dyn std::error::Error>>
{
	// Determine CMake install destination and build type
	let (_cmake_build_type, cmake_install_dest) = match std::env::var("PROFILE")?.as_str() {
		"debug"   => ("Debug", out_dir.join("slang-install")),
		"release" => ("Release", out_dir.join("slang-install")),
		profile => {
			println!("cargo::error=try_build_slang_native: Unknown Cargo profile: {profile}");
			return Err(format!("Unknown Cargo profile: {profile}").into());
		}
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
			let generators_build_path_arg = path_to_str(generators_build_path.as_path())?;
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
			let generators_dir_arg = path_to_str(generators_dir.as_path())?;
			let cmake_result = process::Command::new("cmake")
				.current_dir(slang_path.as_path())
				.args([
					"--install", generators_build_path_arg, "--prefix", generators_dir_arg, "--component", "generators"
				])
				.output()
				.expect("Could not spawn CMake process");
			check_cmake_output(cmake_result)?;

			// emcmake cmake -DSLANG_GENERATORS_PATH=generators/bin --preset emscripten -G "Ninja"
			let generators_dir_option = format!(
				"-DSLANG_GENERATORS_PATH={}", path_to_str(generators_dir.join("bin").as_path())?
			);
			let slang_build_dir =  out_dir.join("slang-build");
			if !slang_build_dir.exists() {
				fs::create_dir(slang_build_dir.as_path()).expect("Failed to create Slang build directory");
			}
			let slang_build_dir_arg = path_to_str(slang_build_dir.as_path())?;
			let cmake_result = process::Command::new("emcmake")
				.current_dir(slang_path.as_path())
				.args([
					"cmake", generators_dir_option.as_str(), "--preset", "emscripten", "-G", "Ninja",
					"-B", slang_build_dir_arg
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
				return Err("WASM build did not result in release artifacts in expected place".into());
			}
			copy_recursively(slang_wasm_release_artifacts_dir, cmake_install_dest.as_path())?;
			slang_lib_type = "static";
		},

		// Native Slang build
		_ => {
			// Build and install into $OUT_DIR
			try_build_slang_native(slang_path.as_path(), cmake_install_dest.as_path())?;
			slang_lib_type = "dylib";
		}
	}

	// Collect install info
	Ok(get_slang_install_at_path(cmake_install_dest, slang_lib_type))
}

/// Custom build steps – build Slang SDK and handle all additional steps required to make it work on WASM.
fn main () -> Result<(), Box<dyn std::error::Error>>
{
	////
	// Preamble

	// Save build script start time for de-confusing Cargo change detection
	let _ = *SCRIPT_START_TIME;

	// Launch VS Code LLDB debugger if it is installed and attach to the build script
	/*let url = format!(
		"vscode://vadimcn.vscode-lldb/launch/config?{{'request':'attach','pid':{}}}", std::process::id()
	);
	if let Ok(result) = std::process::Command::new("code").arg("--open-url").arg(url).output()
	    && result.status.success() {
		std::thread::sleep(std::time::Duration::from_secs(3)); // <- give debugger time to attach
		std::intrinsics::breakpoint();
	}*/

	// Obtain the output directory
	let out_dir = env::var("OUT_DIR")
		.map(PathBuf::from)
		.expect("The output directory must be set by Cargo as an environment variable");

	// Obtain the target directory
	let target_dir = get_cargo_target_dir(out_dir.as_path())?;


	////
	// Get Slang from _somewhere_

	// The first try is always the system Slang
	let is_wasm = env::var("CARGO_CFG_TARGET_ARCH")? == "wasm32";
	let slang_install_option = if !is_wasm {
		use_slang_from_system()?
	}
	else { None };

	// Next attempt: download a binary release from the Slang GitHub repository if the corresponding feature is enabled
	let slang_install_option =
		if slang_install_option.is_none() && !is_wasm && env::var("CARGO_FEATURE_DOWNLOAD_SLANG_BINARIES").is_ok()
		{
			use_downloaded_slang(out_dir.as_path())?
		}
		else { slang_install_option };

	// Final attempt: build from source if the corresponding feature is enabled
	let slang_install_option =
		if slang_install_option.is_none() && env::var("CARGO_FEATURE_BUILD_SLANG_FROM_SOURCE").is_ok()
		{
			use_internally_built_slang(out_dir.as_path())?
		}
		else { slang_install_option };

	// Obtained _some_ Slang install, so we can continue
	let slang_install = slang_install_option.expect(
		format!(
			"Unable to find (or download, or build) a usable Slang installation!{}",
			if is_wasm {"\nNote that for WASM builds, the feature `build_slang_from_source` MUST be used." } else {""}
		).as_str()
	);


	// Copy libs to target dir if requested
	if env::var("CARGO_FEATURE_COPY_LIBS").is_ok()
	{
		// Copy libs
		for entry in fs::read_dir(slang_install.directory.join("lib"))
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
		if !env::var("CARGO_CFG_WINDOWS").is_ok() && env::var("CARGO_CFG_TARGET_ARCH").unwrap() != "wasm32" {
			let link_args = "-Wl,-rpath=$ORIGIN";
			println!("cargo:rustc-link-arg={link_args}");
			println!("cargo:REQUIRED_LINK_ARGS={link_args}");
		}
	}


	////
	// Generate bindings

	// Setup environment
	if env::var("CARGO_CFG_TARGET_ARCH")? == "wasm32" {
		let emclang_path = env::var("EMSDK").map(PathBuf::from)?.join("upstream/bin/clang");
		println!("cargo::warning=EMclang: {}", emclang_path.display());
		unsafe { env::set_var("CLANG_PATH", emclang_path) };
	}

	link_libraries(&slang_install);

	let mut bindgen_builder = bindgen::builder()
		.header(slang_install.include_file.to_str().unwrap())
		.clang_arg("-v")
		.clang_arg("-xc++")
		.clang_arg("-std=c++17")
		.clang_arg(slang_install.include_path_arg);
	if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32" {
		/*let clang_include_path = env::var("EMSDK").map(PathBuf::from)?.join(
			"upstream/lib/clang/21/include"
		).to_string_lossy().into_owned();
		let clang_include_path_arg = format!("-I{clang_include_path}");*/
		bindgen_builder = bindgen_builder/*
			.clang_arg(clang_include_path_arg)
			.detect_include_paths(true);*/
			.clang_arg("--target=x86_64-unknown-linux-gnu");
	}
	bindgen_builder
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

fn link_libraries (slang_install: &SlangInstall)
{
	let lib_dir = slang_install.directory.join("lib");

	if !lib_dir.is_dir() {
		panic!("Couldn't find the `lib` subdirectory in the Slang installation directory.")
	}

	println!("cargo:rustc-link-search=native={}", lib_dir.display());
	println!("cargo:rustc-link-lib={}=slang", slang_install.lib_type);
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
fn pascal_case_from_snake_case(snake_case: &str) -> String
{
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
