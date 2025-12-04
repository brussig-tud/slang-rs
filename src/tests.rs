use crate as slang;


//////
//
// Utilities
//

fn obtain_test_session (global_session: &slang::GlobalSession, search_paths: &[impl AsRef<std::path::Path>])
	-> Option<slang::Session>
{
	// All compiler options are available through this builder.
	let session_options = slang::CompilerOptions::default()
		.optimization(slang::OptimizationLevel::High)
		.matrix_layout_row(true);

	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));

	let targets = [target_desc];
	let search_paths_storage =
		search_paths.iter().map(|s| std::ffi::CString::new(
			s.as_ref().to_string_lossy().as_bytes()
		).unwrap()).collect::<Vec<_>>();
	let search_paths = search_paths_storage.iter().map(|s| s.as_ptr()).collect::<Vec<_>>();

	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&search_paths)
		.options(&session_options);

	global_session.create_session(&session_desc)
}



//////
//
// Actual tests
//

#[test]
fn compile() {
	let global_session = slang::GlobalSession::new().unwrap();

	let search_path = std::ffi::CString::new("shaders").unwrap();

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
	let module = session.load_module("test.slang").unwrap();
	let entry_point = module.find_entry_point_by_name("main").unwrap();

	let program = session
		.create_composite_component_type(&[module.into(), entry_point.into()])
		.unwrap();

	let linked_program = program.link().unwrap();

	// Entry point to the reflection API.
	let reflection = linked_program.layout(0).unwrap();
	assert_eq!(reflection.entry_point_count(), 1);
	assert_eq!(reflection.parameter_count(), 3);

	let shader_bytecode = linked_program.entry_point_code(0, 0).unwrap();
	assert_ne!(shader_bytecode.as_slice().len(), 0);
}

#[cfg(feature="com_impls")]
#[test]
fn com_impls_blob()
{
	// Step 1 - Compile the test shader into an in-memory IR blob
	let (original_ir_bytes, original_bytecode) = {
		let global_session = slang::GlobalSession::new().unwrap();

		let session = obtain_test_session(&global_session, &["shaders"]).unwrap();
		let module = session.load_module("test.slang").unwrap();
		let entry_point = module.find_entry_point_by_name("main").unwrap();

		let program = session
			.create_composite_component_type(&[
				module.clone().into(), // <-- we're keeping a reference to the module for serializing to disk later
				entry_point.into()
			])
			.unwrap();
		let linked_program = program.link().unwrap();

		(module.serialize().unwrap().as_slice().to_owned(),
		 linked_program.entry_point_code(0, 0).unwrap().as_slice().to_owned())
	};

	// Step 2 - Load the serialized IR blob back into a freshly created session through our custom VecBlob
	let (recreated_ir_bytes, recreated_bytecode) = {
		// Work on a fresh global session to better simulate real-life scenarios
		let global_session = slang::GlobalSession::new().unwrap();

		// Fill our ISlangBlob-conformant VecBlob with the IR bytes of the test shader
		let prev_blob = slang::ComPtr::new(slang::com_impls::VecBlob::from_slice(&original_ir_bytes));

		// Obtain a compiler session and load the contents of our custom VecBlob into it
		let session = obtain_test_session(&global_session, &["shaders"]).unwrap();
		let module = session.load_module_from_ir_blob(
			// Module name needs to be the same as the one that resulted from the original filename, otherwise the
			// metadata in the IR blob will be different, and the equality test at the end will fail
			"test.slang",
			// Module path needs to be the same as the one that resulted from the original filename, otherwise the
			// metadata in the IR blob will be different, and the equality test at the end will fail
			"shaders",
			// ISlangBlob pointer
			&prev_blob
		).unwrap();
		let entry_point = module.find_entry_point_by_name("main").unwrap();

		let program = session
			.create_composite_component_type(&[
				module.clone().into(), // <-- we're keeping a reference to the module for comparing to the original
				entry_point.into()
			])
			.unwrap();
		let linked_program = program.link().unwrap();

		(module.serialize().unwrap().as_slice().to_owned(),
		 linked_program.entry_point_code(0, 0).unwrap().as_slice().to_owned())
	};

	// Compare Slang outputs
	assert_eq!(original_ir_bytes, recreated_ir_bytes, "The IR blobs should be identical");
	assert_eq!(original_bytecode, recreated_bytecode, "The compiled programs should be identical");
}
