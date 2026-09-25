use crate::types::ModelError;

pub fn ttl_seconds(raw: &str) -> Result<u64, ModelError> {
    if raw.is_empty() {
        return Err(ModelError::Ttl(raw.into()));
    }
    let mut total = 0u64;
    let mut digits = String::new();
    let mut had_unit = false;
    for c in raw.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        let n = digits
            .parse::<u64>()
            .map_err(|_| ModelError::Ttl(raw.into()))?;
        digits.clear();
        had_unit = true;
        total = total
            .checked_add(
                n.checked_mul(match c {
                    'w' => 604_800,
                    'd' => 86_400,
                    'h' => 3_600,
                    'm' => 60,
                    's' => 1,
                    _ => return Err(ModelError::Ttl(raw.into())),
                })
                .ok_or_else(|| ModelError::Ttl(raw.into()))?,
            )
            .ok_or_else(|| ModelError::Ttl(raw.into()))?;
    }
    if digits.is_empty() && had_unit {
        Ok(total)
    } else {
        Err(ModelError::Ttl(raw.into()))
    }
}

pub fn ttl_text(seconds: u64) -> String {
    let mut n = seconds;
    let d = n / 86400;
    n %= 86400;
    let h = n / 3600;
    n %= 3600;
    let m = n / 60;
    let s = n % 60;
    let mut out = String::new();
    if d > 0 {
        out.push_str(&format!("{d}d"));
    }
    if h > 0 {
        out.push_str(&format!("{h}h"));
    }
    if m > 0 {
        out.push_str(&format!("{m}m"));
    }
    if s > 0 || out.is_empty() {
        out.push_str(&format!("{s}s"));
    }
    out
}
