<div align="center">

# shader-slang
**Rust bindings for the [Slang](https://github.com/shader-slang/slang/) shader language compiler**

</div>

This crate is a fork of [`shader-slang`](https://github.com/FloatyMonkey/slang-rs.git) by
[Lauro Oyen](https://github.com/laurooyen) of [FloatyMonkey](https://github.com/FloatyMonkey).

Supports both the modern compilation and reflection API.


## Purpose

This fork was created to enable plug-and-play use of the *Slang* shading language in Rust projects. No manual
installation of Slang or the Vulkan SDK required! Just enable the `download_slang_binaries` or `build_slang_from_source`
features, and the build script will take care of fetching the latest _supported_ version of *Slang*, either a binary
release, or a source package, depending on which of the two features is enabled. If none of the two features are
enabled, the crate will try to use a system-wide installation in exactly the same way upstream `shader-slang` does.

**_NOTE_**: Currently, neither `download_slang_binaries` nor `build_slang_from_source` work well on Windows and can thus
not be used there by default. You can try your luck with the `force_on_windows` feature that unlocks them. See
[Cargo.toml](https://github.com/brussig-tud/slang-rs/blob/main/Cargo.toml#L42) for more information.


### Secondary features

The *Slang* compilation API makes heavy use of COM interfaces. Upstream `shader-slang` leaves you on your own here and
you need to provide your own implementations if the *Slang* API requires you to pass your own data wrapped in a COM
object. This crate provides fully functional implementations for the most useful of them so you don't have to. By
enabling the `com_impls` feature, the following implementations become available:

* `ISlangBlob`: provided by
   [`com_impls::VecBlob`](https://github.com/brussig-tud/slang-rs/blob/main/src/com_impls/blob.rs#L62). Useful for
   example for deserializing pre-compiled *Slang*-IR modules from disk to feed them into
   `Session::load_module_from_ir_blob`.


### WASM32 support

This crate adds limited support for WASM32 targets. You will not be able to use *shader-slang* directly from within
your WASM32 *Rust* code, unless you use the deprecated `wasm32-unknown-emscripten` target which might not be an option
if you depend on one of the many *Rust* crates that stopped supporting this target. You can however bridge to a
*shader-slang* WASM32 build via JavaScript, which is currently out-of-scope for this project and will remain so for the
foreseeable future.

WASM32 builds requires use of the `build_slang_from_source` feature.


## Example

```rust
let global_session = slang::GlobalSession::new().unwrap();

let search_path = std::ffi::CString::new("shaders/directory").unwrap();

// All compiler options are available through this builder.
let session_options = slang::CompilerOptions::default()
	.optimization(slang::OptimizationLevel::High)
	.matrix_layout_row(true);

let target_desc = slang::TargetDesc::default()
	.format(slang::CompileTarget::Spirv)
	.profile(global_session.find_profile("glsl_450"));

let targets = [target_desc];
let search_paths = [search_path.as_ptr()];

let session_desc = slang::SessionDesc::default()
	.targets(&targets)
	.search_paths(&search_paths)
	.options(&session_options);

let session = global_session.create_session(&session_desc).unwrap();
let module = session.load_module("filename.slang").unwrap();
let entry_point = module.find_entry_point_by_name("main").unwrap();

let program = session
	.create_composite_component_type(&[module.into(), entry_point.into()])
	.unwrap();

let linked_program = program.link().unwrap();

// Entry point to the reflection API.
let reflection = linked_program.layout(0).unwrap();

let shader_bytecode = linked_program.entry_point_code(0, 0).unwrap();
```

## Installation

Add `shader-slang` to the `[dependencies]` section of your `Cargo.toml`.

Point this library to a Slang installation. An easy way is by installing the [LunarG Vulkan SDK](https://vulkan.lunarg.com) which comes bundled with the Slang compiler. During installation `VULKAN_SDK` is added to the `PATH` and automatically picked up by this library.

Alternatively, download Slang from their [releases page](https://github.com/shader-slang/slang/releases) and manually set the `SLANG_DIR` environment variable to the path of your Slang directory. Copy `slang.dll` to your executable's directory. To compile to DXIL bytecode, also copy `dxil.dll` and `dxcompiler.dll` from the [Microsoft DirectXShaderCompiler](https://github.com/microsoft/DirectXShaderCompiler/releases) to your executable's directory.

To specify the `include` and `lib` directories separately, set the `SLANG_INCLUDE_DIR` and `SLANG_LIB_DIR` environment variables.

## Credits

Maintained by Lauro Oyen ([@laurooyen](https://github.com/laurooyen)).

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
