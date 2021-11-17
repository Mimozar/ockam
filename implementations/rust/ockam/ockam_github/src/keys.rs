use ockam_core::Result;

pub struct Keys;

impl Keys {
    pub fn extract_raw_ed25519_secret_key(key_str: &str) -> Result<Vec<u8>> {
        let lines: Vec<&str> = key_str.lines().collect();

        let first = lines.first().unwrap();
        let last = lines.last().unwrap();

        let mut key_str = "".to_string();
        for line in &lines[1..lines.len() - 1] {
            key_str.push_str(line);
        }

        if first != &"-----BEGIN OPENSSH PRIVATE KEY-----" {
            panic!()
        }

        if last != &"-----END OPENSSH PRIVATE KEY-----" {
            panic!()
        }

        let key_data = base64::decode(key_str).unwrap(); // FIXME

        let key_data = key_data[161..193].to_vec();

        Ok(key_data)
    }

    pub fn extract_raw_ed25519_public_key(key_str: &str) -> Result<Vec<u8>> {
        let mut split = key_str.split_whitespace();

        if let Some(kt) = split.next() {
            if kt != "ssh-ed25519" {
                panic!()
            }
        } else {
            panic!()
        }

        let key_str;
        if let Some(ks) = split.next() {
            key_str = ks;
        } else {
            panic!()
        }

        let key_data = base64::decode(key_str).unwrap(); // FIXME

        // FIXME: MAGIC NUMBER
        if key_data.len() != 51 {
            panic!()
        }

        let key_data = key_data[19..].to_vec();

        Ok(key_data)
    }
}

#[cfg(test)]
mod tests {
    use crate::Keys;
    use ockam_vault::ockam_vault_core::{SecretPersistence, SecretType};
    use ockam_vault::{PublicKey, SecretAttributes, SecretVault, Signer, SoftwareVault, Verifier};

    const VALID_SECRET_KEY: &'static str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACD8wfh3Dam8lP1avwWXpFbLCZIuL3BlAgz+gYDKxiPERgAAAKB4l3KgeJdy
oAAAAAtzc2gtZWQyNTUxOQAAACD8wfh3Dam8lP1avwWXpFbLCZIuL3BlAgz+gYDKxiPERg
AAAECJ7gnmFRfhIuAYmL+TXjW8GTZ6G9DuRzk2IA4cCwz9r/zB+HcNqbyU/Vq/BZekVssJ
ki4vcGUCDP6BgMrGI8RGAAAAFnlvdXJfZW1haWxAZXhhbXBsZS5jb20BAgMEBQYH
-----END OPENSSH PRIVATE KEY-----";

    const VALID_PUBLIC_KEY: &'static str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPzB+HcNqbyU/Vq/BZekVssJki4vcGUCDP6BgMrGI8RG your_email@example.com";

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn extract_keys__correct_key_pair__should_succeed() {
        let secret_key_data = Keys::extract_raw_ed25519_secret_key(VALID_SECRET_KEY).unwrap();
        let public_key_data = Keys::extract_raw_ed25519_public_key(VALID_PUBLIC_KEY).unwrap();

        let mut vault = SoftwareVault::default();

        let secret = vault
            .secret_import(
                &secret_key_data,
                SecretAttributes::new(SecretType::Ed25519, SecretPersistence::Ephemeral, 32),
            )
            .await
            .unwrap();

        let msg = b"TEST";
        let signature = vault.sign(&secret, msg).await.unwrap();
        let public = PublicKey::new(public_key_data, SecretType::Ed25519);

        let res = vault.verify(&signature, &public, msg).await.unwrap();

        assert!(res)
    }
}
