/// Holds the study statistics for a single card and computes its score.
/// Isolated from card content so the algorithm can change independently.
pub struct Score {
    pub times_seen: u32,
    pub times_correct: u32,
}

impl Score {
    pub fn new(times_seen: u32, times_correct: u32) -> Self {
        Self { times_seen, times_correct }
    }

    /// Returns a 0–100 confidence percentage.
    ///
    /// Formula: exponential curve applied to (correct - 2 * incorrect).max(0).
    /// A wrong answer penalises more than a right answer rewards.
    pub fn progress_percent(&self) -> u8 {
        let good = self.times_correct as f64;
        let bad = self.times_seen.saturating_sub(self.times_correct) as f64;

        if good == 0.0 {
            return 0;
        }

        let alpha = 2.0; // penalty multiplier for wrong answers
        let k = 0.6;     // curve steepness

        let score = (good - alpha * bad).max(0.0);
        let confidence = 1.0 - (-k * score).exp();
        (confidence * 100.0).round().clamp(0.0, 100.0) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_attempts_returns_zero() {
        assert_eq!(Score::new(0, 0).progress_percent(), 0);
    }

    #[test]
    fn more_correct_answers_increases_score() {
        let low = Score::new(5, 5).progress_percent();
        let high = Score::new(10, 10).progress_percent();
        assert!(high > low);
    }

    #[test]
    fn wrong_answers_drag_score_down() {
        let all_good = Score::new(10, 10).progress_percent();
        let mixed = Score::new(10, 6).progress_percent();
        assert!(all_good > mixed);
    }

    #[test]
    fn score_never_exceeds_100() {
        assert!(Score::new(1000, 1000).progress_percent() <= 100);
    }
}
