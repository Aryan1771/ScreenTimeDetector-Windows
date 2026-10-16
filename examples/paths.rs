use std::os::windows::io::AsRawHandle;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFinalPathNameByHandleW(h: *mut std::ffi::c_void, b: *mut u16, n: u32, f: u32) -> u32;
}
fn main() {
    for p in [
        r"\\?\C:\Users\aryan\AppData\Roaming\DigitalWellbeing\DigitalWellbeing\data\wellbeing.sqlite3",
        r"C:\Users\aryan\AppData\Local\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Roaming\DigitalWellbeing\DigitalWellbeing\data\wellbeing.sqlite3",
    ] {
        let f = std::fs::File::open(p).unwrap();
        let mut b = vec![0u16; 32768];
        let n = unsafe {
            GetFinalPathNameByHandleW(f.as_raw_handle(), b.as_mut_ptr(), b.len() as u32, 0)
        };
        println!("{} => {}", p, String::from_utf16_lossy(&b[..n as usize]));
        let c = rusqlite::Connection::open(p).unwrap();
        println!(
            "{}",
            c.query_row(
                "SELECT group_concat(domain||'='||active_seconds) FROM website_usage",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap()
        );
    }
}
