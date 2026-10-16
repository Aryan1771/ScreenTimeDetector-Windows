#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentPackageFullName(len: *mut u32, name: *mut u16) -> i32;
}
fn main() {
    let mut len = 0;
    let r = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    println!("package result {r} len {len}");
    if len > 0 {
        let mut name = vec![0; len as usize];
        unsafe {
            GetCurrentPackageFullName(&mut len, name.as_mut_ptr());
        }
        println!("{}", String::from_utf16_lossy(&name));
    }
}
