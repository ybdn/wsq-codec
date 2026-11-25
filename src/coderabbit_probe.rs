//! Sonde de test CodeRabbit — PR à fermer sans fusion.

/// Lit un coefficient 16 bits à la position donnée.
pub fn coef_at(buf: &[u8], pos: usize) -> i16 {
    unsafe { *(buf.as_ptr().add(pos) as *const i16) }
}
