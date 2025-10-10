
//////
//
// Imports
//

// Standard library
use std::{ffi::c_void, mem::ManuallyDrop, sync::atomic::{AtomicU32, Ordering}};

// Local imports
use crate::{*, com_impls::*};



//////
//
// Traits
//

/// The trait of implementing [`ISlangBlob`](sys::ISlangBlob). If the `com_impls` feature is enabled, then the standard
/// [`Blob`] type will also implement this trait.
pub trait ImplementsISlangBlob: Interface
{
	#[inline(always)]
	fn as_slice (&self) -> &[u8] {
		unsafe {
			// SAFETY: The ISlangBlob interface guarantees valid buffer parameters
			std::slice::from_raw_parts(
				self.get_buffer_pointer() as *const u8, self.get_buffer_size()
			)
		}
	}

	fn get_buffer_pointer (&self) -> *const c_void;

	fn get_buffer_size(&self) -> usize;
}
impl ImplementsISlangBlob for Blob {
	#[inline(always)]
	fn as_slice(&self) -> &[u8] {
		self.as_slice()
	}

	#[inline(always)]
	fn get_buffer_pointer(&self) -> *const c_void {
		vcall!(self, getBufferPointer())
	}

	#[inline(always)]
	fn get_buffer_size(&self) -> usize {
		vcall!(self, getBufferSize())
	}
}



//////
//
// Structs
//

/// A pure Rust implementation of [`ISlangBlob`](sys::ISlangBlob) that uses a `Vec<u8>` as its backing store.
#[repr(C)]
pub struct VecBlob {
	/// The VTable binding the COM interface to our struct.
	vtable_: *const sys::IBlobVtable,

	/// We implement reference counting using *Rust* atomics.
	ref_count: AtomicU32,

	/// The actual blob.
	data: Vec<u8>,
}
impl VecBlob
{
	///
	pub fn from_vec (data: Vec<u8>) -> *mut VecBlob {
		// Allocate our object and return it casted to ISlangBlob pointer type
		let mut boxed = Box::new(VecBlob {
			vtable_: &VTABLE,
			ref_count: AtomicU32::new(1),
			data,
		});
		let ptr: *mut VecBlob = &mut *boxed;
		// We must not drop the Box; transfer ownership to COM. Use ManuallyDrop.
		let _ = ManuallyDrop::new(boxed);
		ptr
	}

	///
	pub fn from_slice (data: &[u8]) -> *mut VecBlob {
		Self::from_vec(data.to_owned())
	}

	///
	pub fn from_string (s: String) -> *mut VecBlob {
		Self::from_vec(s.into_bytes())
	}

	///
	pub fn from_str (s: &str) -> *mut VecBlob {
		Self::from_vec(s.as_bytes().to_owned())
	}

	#[inline]
	fn this<'a>(this: *mut sys::ISlangUnknown) -> &'a mut VecBlob {
		// Safety: our object layout is compatible; the incoming pointer is one we created.
		unsafe { &mut *(this as *mut VecBlob) }
	}

	#[inline]
	fn this_void<'a>(this: *mut c_void) -> &'a mut VecBlob {
		unsafe { &mut *(this as *mut VecBlob) }
	}
}
unsafe impl Interface for VecBlob {
	type Vtable = sys::IBlobVtable;
	const IID: UUID = uuid(
		0x8ba5fb08,
		0x5195,
		0x40e2,
		[0xac, 0x58, 0x0d, 0x98, 0x9c, 0x3a, 0x01, 0x02],
	);

	#[inline(always)]
	unsafe fn as_raw<T>(&self) -> *mut T {
		self as *const Self as *mut T
	}
}
impl ImplementsISlangBlob for VecBlob {
	#[inline(always)]
	fn get_buffer_pointer (&self) -> *const std::ffi::c_void {
		self.data.as_ptr() as *const std::ffi::c_void
	}

	#[inline(always)]
	fn get_buffer_size(&self) -> usize {
		self.data.len()
	}
}



//////
//
// COM endpoint implementations
//

////
// Interface: IUnknown

unsafe extern "C" fn query_interface (
	this: *mut sys::ISlangUnknown,
	uuid: *const sys::SlangUUID,
	out_object: *mut *mut c_void,
) -> sys::SlangResult {
	if out_object.is_null() || uuid.is_null() {
		return E_INVALIDARG;
	}
	let obj = VecBlob::this(this);

	let iid = unsafe { &*uuid };
	let mut matched: Option<*mut c_void> = None;

	if eq_guid(iid, &IUnknown::IID) || eq_guid(iid, &VecBlob::IID) {
		// We can return ourselves for both IUnknown and ISlangBlob
		matched = Some(obj as *mut VecBlob as *mut c_void);
	}

	if let Some(ptr) = matched {
		// Increase refcount for the returned interface
		obj.ref_count.fetch_add(1, Ordering::Relaxed);
		unsafe { *out_object = ptr; }
		S_OK
	}
	else {
		unsafe { *out_object = std::ptr::null_mut() };
		// SLANG_E_NO_INTERFACE
		E_NOINTERFACE
	}
}

unsafe extern "C" fn add_ref (this: *mut sys::ISlangUnknown) -> u32 {
	let obj = VecBlob::this(this);
	let prev = obj.ref_count.fetch_add(1, Ordering::Relaxed);
	prev + 1
}

unsafe extern "C" fn release (this: *mut sys::ISlangUnknown) -> u32
{
	let obj = VecBlob::this(this);
	let prev = obj.ref_count.fetch_sub(1, Ordering::Release);
	if prev == 1 {
		// Acquire to synchronize with potential writers before drop
		std::sync::atomic::fence(Ordering::Acquire);
		// Reconstruct the Box and drop
		let _ = unsafe {
			// Safety: we own the Box, and the Box is the only reference to it.
			Box::from_raw(obj as *mut VecBlob)
		};
		0
	} else {
		prev-1
	}
}


////
// Interface: ISlangBlob

unsafe extern "C" fn get_buffer_pointer(this: *mut c_void) -> *const c_void {
	let obj = VecBlob::this_void(this);
	obj.data.as_ptr() as *const c_void
}

unsafe extern "C" fn get_buffer_size(this: *mut c_void) -> usize {
	let obj = VecBlob::this_void(this);
	obj.data.len()
}


////
// Interface binding

static VTABLE: sys::IBlobVtable = sys::IBlobVtable {
	_base: sys::ISlangUnknown__bindgen_vtable {
		ISlangUnknown_queryInterface: query_interface,
		ISlangUnknown_addRef: add_ref,
		ISlangUnknown_release: release,
	},
	getBufferPointer: get_buffer_pointer,
	getBufferSize: get_buffer_size,
};
