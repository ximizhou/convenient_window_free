use anyhow::{ensure, Result};

pub(crate) fn adjusted_brightness(min: u32, current: u32, max: u32, delta: f32) -> Result<u32> {
    ensure!(
        min < max && (min..=max).contains(&current),
        "invalid brightness range"
    );
    ensure!(
        delta.is_finite() && delta != 0.0,
        "invalid brightness delta"
    );
    let step = (f64::from(max - min) * f64::from(delta)).round();
    let step = if delta > 0.0 {
        step.max(1.0)
    } else {
        step.min(-1.0)
    };
    Ok((f64::from(current) + step).clamp(f64::from(min), f64::from(max)) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn brightness_steps_use_the_device_range_and_stop_at_its_limits() {
        assert_eq!(adjusted_brightness(0, 50, 100, 0.05).unwrap(), 55);
        assert_eq!(adjusted_brightness(20, 100, 220, -0.05).unwrap(), 90);
        assert_eq!(adjusted_brightness(20, 215, 220, 0.05).unwrap(), 220);
        assert_eq!(adjusted_brightness(20, 21, 220, -0.05).unwrap(), 20);
        assert_eq!(adjusted_brightness(0, 5, 10, 0.001).unwrap(), 6);
        assert!(adjusted_brightness(10, 5, 100, 0.05).is_err());
        assert!(adjusted_brightness(10, 10, 10, 0.05).is_err());
        assert!(adjusted_brightness(0, 50, 100, f32::NAN).is_err());
    }
}
