use std::fmt;

/// Authentication scope types for type-safe cookie and session management
///
/// This enum replaces magic strings in authentication code with type-safe
/// variants that provide centralized cookie formatting and security attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthScope {
    /// Primary session authentication (email login, WebAuthn)
    /// Duration: 7 days
    /// Cookie name: "auth"
    Session,

    /// Temporary redirect state during login flow
    /// Duration: 10 minutes
    /// Cookie name: "redirect_state"
    RedirectState,
}

impl AuthScope {
    /// Get the cookie name for this authentication scope
    pub fn cookie_name(self) -> &'static str {
        match self {
            AuthScope::Session => "auth",
            AuthScope::RedirectState => "redirect_state",
        }
    }

    /// Get the maximum age as a Duration
    pub fn duration(self) -> std::time::Duration {
        match self {
            AuthScope::Session => std::time::Duration::from_secs(604800), // 7 days
            AuthScope::RedirectState => std::time::Duration::from_secs(3600), // 1 hour
        }
    }

    /// Check if this scope requires secure-only cookies (HTTPS)
    pub fn is_secure_only(self) -> bool {
        match self {
            AuthScope::Session => true, // Session cookies should be secure in production
            AuthScope::RedirectState => false, // Redirect state can work over HTTP for development
        }
    }

    /// Check if this scope uses signed cookies
    pub fn is_signed(self) -> bool {
        match self {
            AuthScope::Session => true,        // Session cookies are HMAC signed
            AuthScope::RedirectState => false, // Redirect state is just URL encoded
        }
    }

    /// Create a properly formatted cookie string for this scope
    pub fn format_cookie(self, value: &str, use_secure: bool) -> String {
        let secure = if use_secure && self.is_secure_only() {
            "; Secure"
        } else {
            ""
        };

        format!(
            "{}={}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{}",
            self.cookie_name(),
            value,
            self.duration().as_secs(),
            secure
        )
    }

    /// Create a cookie clearing string for this scope
    pub fn clear_cookie(self, use_secure: bool) -> String {
        let secure = if use_secure && self.is_secure_only() {
            "; Secure"
        } else {
            ""
        };

        format!(
            "{}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}",
            self.cookie_name(),
            secure
        )
    }

    /// Parse from cookie name string
    pub fn from_cookie_name(name: &str) -> Option<AuthScope> {
        match name {
            "auth" => Some(AuthScope::Session),
            "redirect_state" => Some(AuthScope::RedirectState),
            _ => None,
        }
    }
}

impl fmt::Display for AuthScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthScope::Session => write!(f, "session"),
            AuthScope::RedirectState => write!(f, "redirect_state"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_scope_functionality() {
        // Test cookie names
        assert_eq!(AuthScope::Session.cookie_name(), "auth");
        assert_eq!(AuthScope::RedirectState.cookie_name(), "redirect_state");

        // Test durations
        assert_eq!(AuthScope::Session.duration().as_secs(), 604800); // 7 days
        assert_eq!(AuthScope::RedirectState.duration().as_secs(), 3600); // 1 hour

        // Test security requirements
        assert!(AuthScope::Session.is_secure_only());
        assert!(!AuthScope::RedirectState.is_secure_only());

        // Test signing requirements
        assert!(AuthScope::Session.is_signed());
        assert!(!AuthScope::RedirectState.is_signed());

        // Test parsing from cookie names
        assert_eq!(
            AuthScope::from_cookie_name("auth"),
            Some(AuthScope::Session)
        );
        assert_eq!(
            AuthScope::from_cookie_name("redirect_state"),
            Some(AuthScope::RedirectState)
        );
        assert_eq!(AuthScope::from_cookie_name("invalid"), None);
        assert_eq!(AuthScope::from_cookie_name(""), None);

        // Test display
        assert_eq!(format!("{}", AuthScope::Session), "session");
        assert_eq!(format!("{}", AuthScope::RedirectState), "redirect_state");
    }

    #[test]
    fn test_auth_scope_cookie_formatting() {
        // Test session cookie formatting (without secure flag)
        let cookie = AuthScope::Session.format_cookie("test_value", false);
        assert_eq!(
            cookie,
            "auth=test_value; Path=/; Max-Age=604800; HttpOnly; SameSite=Lax"
        );

        // Test session cookie formatting (with secure flag)
        let secure_cookie = AuthScope::Session.format_cookie("test_value", true);
        assert_eq!(
            secure_cookie,
            "auth=test_value; Path=/; Max-Age=604800; HttpOnly; SameSite=Lax; Secure"
        );

        // Test redirect state cookie (should not have secure flag even when requested)
        let redirect_cookie = AuthScope::RedirectState.format_cookie("http://example.com", true);
        assert_eq!(
            redirect_cookie,
            "redirect_state=http://example.com; Path=/; Max-Age=3600; HttpOnly; SameSite=Lax"
        );
    }

    #[test]
    fn test_auth_scope_cookie_clearing() {
        // Test session cookie clearing (without secure)
        let clear_cookie = AuthScope::Session.clear_cookie(false);
        assert_eq!(
            clear_cookie,
            "auth=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"
        );

        // Test session cookie clearing (with secure)
        let secure_clear = AuthScope::Session.clear_cookie(true);
        assert_eq!(
            secure_clear,
            "auth=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax; Secure"
        );

        // Test redirect state clearing (should not have secure even when requested)
        let redirect_clear = AuthScope::RedirectState.clear_cookie(true);
        assert_eq!(
            redirect_clear,
            "redirect_state=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"
        );
    }
}
