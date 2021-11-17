#[derive(Clone, Copy, Debug)]
pub enum GithubError {
    HttpError = 1,
}

impl GithubError {
    /// Integer code associated with the error domain.
    pub const DOMAIN_CODE: u32 = 22_000;
    /// Descriptive name for the error domain.
    pub const DOMAIN_NAME: &'static str = "OCKAM_GITHUB";
}

impl From<GithubError> for ockam_core::Error {
    fn from(e: GithubError) -> ockam_core::Error {
        ockam_core::Error::new(
            GithubError::DOMAIN_CODE + (e as u32),
            GithubError::DOMAIN_NAME,
        )
    }
}
