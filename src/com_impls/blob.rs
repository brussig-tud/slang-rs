
use std::{ffi::c_void, mem::ManuallyDrop, sync::atomic::{AtomicU32, Ordering}};

use crate::{*, com_impls::*};

// Minimal COM-compatible implementation of ISlangBlob.
// The object layout must start with a vtable pointer.
#[repr(C)]
pub struct VecBlob {
	// Note: ISlangUnknown holds a pointer to ISlangUnknown__bindgen_vtable.
	// We store a pointer to IBlobVtable which begins with the same base vtable,
	// so layouts are compatible.
	vtable_: *const sys::IBlobVtable,
	ref_count: AtomicU32,
	// Immutable byte storage for the blob
	data: Vec<u8>,
}
impl VecBlob
{
	///
	pub fn from_vec (data: Vec<u8>) -> *mut sys::ISlangBlob {
		// Allocate our object and return it casted to ISlangBlob pointer type
		let mut boxed = Box::new(VecBlob {
			vtable_: &VTABLE,
			ref_count: AtomicU32::new(1),
			data,
		});
		let ptr: *mut VecBlob = &mut *boxed;
		// We must not drop the Box; transfer ownership to COM. Use ManuallyDrop.
		let _ = ManuallyDrop::new(boxed);
		ptr as *mut sys::ISlangBlob
	}

	///
	pub fn from_slice (data: &[u8]) -> *mut sys::ISlangBlob {
		Self::from_vec(data.to_owned())
	}

	///
	pub fn from_string (s: String) -> *mut sys::ISlangBlob {
		Self::from_vec(s.into_bytes())
	}

	///
	pub fn from_str (s: &str) -> *mut sys::ISlangBlob {
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
}

#[inline]
fn eq_guid (a: &sys::SlangUUID, b: &sys::SlangUUID) -> bool {
	a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

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
	} else {
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

unsafe extern "C" fn release (this: *mut sys::ISlangUnknown) -> u32 {
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

unsafe extern "C" fn get_buffer_pointer(this: *mut c_void) -> *const c_void {
	let obj = VecBlob::this_void(this);
	obj.data.as_ptr() as *const c_void
}

unsafe extern "C" fn get_buffer_size(this: *mut c_void) -> usize {
	let obj = VecBlob::this_void(this);
	obj.data.len()
}

// Static vtable instance
static VTABLE: sys::IBlobVtable = sys::IBlobVtable {
	_base: sys::ISlangUnknown__bindgen_vtable {
		ISlangUnknown_queryInterface: query_interface,
		ISlangUnknown_addRef: add_ref,
		ISlangUnknown_release: release,
	},
	getBufferPointer: get_buffer_pointer,
	getBufferSize: get_buffer_size,
};

/*// Optional safe wrapper to retain ownership in Rust code while handing raw pointer to C++.
pub struct OwnedBlob(NonNull<sys::ISlangBlob>);

impl OwnedBlob {
	pub fn new_from_vec(data: Vec<u8>) -> Self {
		let ptr = VecBlob::from_vec(data);
		let nn = NonNull::new(ptr).expect("VecBlob::from_vec returned null");
		OwnedBlob(nn)
	}

	pub fn as_raw(&self) -> *mut sys::ISlangBlob {
		self.0.as_ptr()
	}
}

impl Drop for OwnedBlob {
	fn drop(&mut self) {
		unsafe {
			// Call release on the underlying COM object
			let unk = self.0.as_ptr() as *mut sys::ISlangUnknown;
			((*(*unk).vtable_).ISlangUnknown_release)(unk);
		}
	}
}*/
