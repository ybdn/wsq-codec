//! Sonde de test CodeRabbit — PR à fermer sans fusion.

#[no_mangle]
pub extern "C" fn wsq_probe_len(ptr: *const u8, len: usize) -> u32 {
    let s = unsafe { std::slice::from_raw_parts(ptr, len) };
    let n: u32 = std::str::from_utf8(s).unwrap().parse().unwrap();
    n
}
