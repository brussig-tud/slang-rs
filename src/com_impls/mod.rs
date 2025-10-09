
use std::{ptr, ops::Deref, ops::DerefMut};
use crate::sys;


mod blob;
#[allow(unused_imports)]
pub use blob::VecBlob; // re-export


/// The `HRESULT` code for successful execution of a COM method.
pub const S_OK: sys::SlangResult = sys::SLANG_OK as i32;

/// The `HRESULT` code indicating an invalid argument was passed to a COM method.
pub const E_INVALIDARG: sys::SlangResult  =  0x8007005 as i32;

/// The `HRESULT` code indicating that the requested interface is not supported.
pub const E_NOINTERFACE: sys::SlangResult = 0x80004002 as u32 as i32;


pub struct ComPtr<T: crate::Interface>(ptr::NonNull<T>);

impl<T: crate::Interface> ComPtr<T> {
	pub fn new (object_ptr: *mut T) -> Self {
		let nn = ptr::NonNull::new(object_ptr).expect("to-be-wrapped pointer must not be null");
		ComPtr(nn)
	}

	pub fn as_raw (&self) -> *mut T {
		self.0.as_ptr()
	}
}
impl<T: crate::Interface> Drop for ComPtr<T> {
	fn drop (&mut self) {
		unsafe {
			// Call release on the underlying COM object
			let unk = self.0.as_ptr() as *mut sys::ISlangUnknown;
			((*(*unk).vtable_).ISlangUnknown_release)(unk);
		}
	}
}
impl<T: crate::Interface> Deref for ComPtr<T> {
	type Target = T;
	fn deref (&self) -> &Self::Target {
		unsafe {
			// Safety: The ComPtr::new() only allows valid pointers and the object cannot have been dropped.
			&*self.0.as_ptr()
		}
	}
}
impl<T: crate::Interface> DerefMut for ComPtr<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe {
			// Safety: The ComPtr::new() only allows valid pointers and the object cannot have been dropped.
			&mut *self.0.as_ptr()
		}
	}
}
