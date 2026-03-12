/// Sliding-window anti-replay mechanism.
///
/// Uses a 1024-bit bitmap to track recently seen nonce counter values.
/// Any nonce below `base` (i.e. more than WINDOW_SIZE behind the highest seen)
/// is rejected as too old. Any nonce already seen within the window is rejected.
pub struct AntiReplayWindow {
    bitmap: [u64; Self::BITMAP_WORDS],
    base: u64,
}

impl AntiReplayWindow {
    const WINDOW_SIZE: u64 = 1024;
    const BITMAP_WORDS: usize = (Self::WINDOW_SIZE / 64) as usize; // 16 words

    pub fn new() -> Self {
        Self {
            bitmap: [0u64; Self::BITMAP_WORDS],
            base: 0,
        }
    }

    /// Check if a nonce counter value is valid (not replayed) and record it.
    /// Returns Ok(()) if the nonce is fresh, Err if replayed or too old.
    pub fn check_and_update(&mut self, counter: u64) -> Result<(), crate::error::ProtocolError> {
        if counter < self.base {
            return Err(crate::error::ProtocolError::ReplayDetected(counter));
        }

        let offset = counter - self.base;

        if offset >= Self::WINDOW_SIZE {
            // Advance the window
            let shift = offset - Self::WINDOW_SIZE + 1;
            self.advance(shift);
        }

        let idx = ((counter - self.base) / 64) as usize;
        let bit = (counter - self.base) % 64;

        if idx >= Self::BITMAP_WORDS {
            return Err(crate::error::ProtocolError::ReplayDetected(counter));
        }

        if self.bitmap[idx] & (1u64 << bit) != 0 {
            return Err(crate::error::ProtocolError::ReplayDetected(counter));
        }

        self.bitmap[idx] |= 1u64 << bit;
        Ok(())
    }

    fn advance(&mut self, shift: u64) {
        if shift >= Self::WINDOW_SIZE {
            // Entire window is invalidated
            self.bitmap = [0u64; Self::BITMAP_WORDS];
            self.base += shift;
            return;
        }

        let word_shift = (shift / 64) as usize;
        let bit_shift = (shift % 64) as u32;

        if word_shift > 0 {
            self.bitmap.rotate_left(word_shift);
            for w in &mut self.bitmap[Self::BITMAP_WORDS - word_shift..] {
                *w = 0;
            }
        }

        if bit_shift > 0 {
            let mut carry = 0u64;
            for w in self.bitmap.iter_mut().rev() {
                let new_carry = *w << (64 - bit_shift);
                *w = (*w >> bit_shift) | carry;
                carry = new_carry;
            }
        }

        self.base += shift;
    }
}

impl Default for AntiReplayWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_nonces() {
        let mut window = AntiReplayWindow::new();
        for i in 0..100 {
            assert!(window.check_and_update(i).is_ok(), "nonce {} rejected", i);
        }
    }

    #[test]
    fn test_replay_detected() {
        let mut window = AntiReplayWindow::new();
        assert!(window.check_and_update(5).is_ok());
        assert!(window.check_and_update(5).is_err()); // replay
    }

    #[test]
    fn test_old_nonce_rejected() {
        let mut window = AntiReplayWindow::new();
        // Advance past the window
        assert!(window.check_and_update(2000).is_ok());
        // Old nonce should be rejected
        assert!(window.check_and_update(0).is_err());
    }

    #[test]
    fn test_out_of_order_within_window() {
        let mut window = AntiReplayWindow::new();
        assert!(window.check_and_update(10).is_ok());
        assert!(window.check_and_update(5).is_ok()); // within window, not seen
        assert!(window.check_and_update(8).is_ok());
        assert!(window.check_and_update(5).is_err()); // already seen
    }

    #[test]
    fn test_window_advance() {
        let mut window = AntiReplayWindow::new();
        for i in 0..500 {
            assert!(window.check_and_update(i).is_ok());
        }
        // Jump ahead
        assert!(window.check_and_update(2000).is_ok());
        // Old values should be rejected
        assert!(window.check_and_update(400).is_err());
        // Values near the new high should work
        assert!(window.check_and_update(1999).is_ok());
    }
}
