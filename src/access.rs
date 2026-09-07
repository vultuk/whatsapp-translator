//! Startup access policy and a shared password-attempt budget for this single-user service.
use std::{
    net::IpAddr,
    time::{Duration, Instant},
};

pub fn validate(
    host: &str,
    password: Option<&str>,
    allow_local_no_auth: bool,
) -> anyhow::Result<()> {
    let local = host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if password.is_some_and(|value| value.trim().is_empty()) {
        anyhow::bail!("WA_PASSWORD must not be empty or whitespace.");
    }
    if allow_local_no_auth && !local {
        anyhow::bail!(
            "WA_ALLOW_LOCAL_NO_AUTH is only permitted with a loopback WA_HOST (127.0.0.1 or ::1)."
        );
    }
    if password.is_none() && !(local && allow_local_no_auth) {
        anyhow::bail!("Set WA_PASSWORD before starting the web server. For local development only, set WA_HOST=127.0.0.1 and WA_ALLOW_LOCAL_NO_AUTH=true.");
    }
    Ok(())
}

pub struct LoginBudget {
    start: Instant,
    used: u32,
}
impl Default for LoginBudget {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            used: 0,
        }
    }
}
impl LoginBudget {
    pub fn take(&mut self) -> bool {
        self.take_at(Instant::now())
    }
    fn take_at(&mut self, now: Instant) -> bool {
        if now.duration_since(self.start) >= Duration::from_secs(60) {
            self.start = now;
            self.used = 0;
        }
        if self.used >= 15 {
            return false;
        }
        self.used += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosted_access_cannot_bypass_authentication() {
        for host in ["0.0.0.0", "::", "192.168.1.10", "localhost", "example.com"] {
            assert!(validate(host, None, false).is_err());
            assert!(validate(host, None, true).is_err());
        }
        assert!(validate("0.0.0.0", Some("secret"), false).is_ok());
        for host in ["127.0.0.1", "::1"] {
            assert!(validate(host, None, false).is_err());
            assert!(validate(host, None, true).is_ok());
        }
        assert!(validate("127.0.0.1", Some("  "), true).is_err());
    }
    #[test]
    fn login_budget_is_bounded_and_recovers() {
        let mut budget = LoginBudget::default();
        let now = budget.start;
        for _ in 0..15 {
            assert!(budget.take_at(now));
        }
        assert!(!budget.take_at(now));
        assert!(budget.take_at(now + Duration::from_secs(60)));
    }
}
