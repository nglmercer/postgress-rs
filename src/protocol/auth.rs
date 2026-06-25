use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethod {
    Trust,
    Md5,
    ScramSha256,
}

#[derive(Debug, Clone)]
pub struct AuthState {
    pub method: AuthMethod,
    pub salt: [u8; 4],
    pub server_nonce: String,
    pub iterations: u32,
}

impl AuthState {
    pub fn start_auth(method: AuthMethod) -> Self {
        let mut salt = [0u8; 4];
        for i in 0..4 {
            salt[i] = (i as u8).wrapping_mul(7);
        }

        Self {
            method,
            salt,
            server_nonce: format!("{:016x}", 1234567890u64),
            iterations: 4096,
        }
    }

    pub fn verify_md5(&self, password_hash: &[u8], user: &str, password: &str) -> bool {
        let expected = format!("md5{:032x}", md5_hash(&format!("{}{}", password, user)));
        password_hash == expected.as_bytes()
    }

    pub fn verify_scram(&self, client_response: &ScramClientFinal, password: &str) -> bool {
        let salted_password = pbkdf2(password, &self.salt, self.iterations);
        let client_key = hmac_sha256(&salted_password, b"Client Key");
        let stored_key = sha256(&client_key);

        let auth_message = format!(
            "n,,{},{}",
            client_response.auth_message.split(',').nth(1).unwrap_or(""),
            client_response.auth_message.split(',').nth(2).unwrap_or("")
        );

        let client_signature = hmac_sha256(&stored_key, auth_message.as_bytes());
        let client_proof = xor(&client_key, &client_signature);

        client_proof == client_response.client_proof
    }
}

#[derive(Debug, Clone)]
pub struct ScramClientFirst {
    pub username: String,
    pub client_nonce: String,
}

#[derive(Debug, Clone)]
pub struct ScramServerFirst {
    pub salt: Vec<u8>,
    pub iterations: u32,
    pub server_nonce: String,
}

#[derive(Debug, Clone)]
pub struct ScramClientFinal {
    pub auth_message: String,
    pub client_proof: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct UserStore {
    users: HashMap<String, String>,
}

impl UserStore {
    pub fn new() -> Self {
        Self { users: HashMap::new() }
    }

    pub fn add_user(&mut self, username: &str, password: &str) {
        let hash = format!("md5{:032x}", md5_hash(&format!("{}{}", password, username)));
        self.users.insert(username.to_string(), hash);
    }

    pub fn verify_user(&self, username: &str, password: &str) -> bool {
        if let Some(stored_hash) = self.users.get(username) {
            let computed = format!("md5{:032x}", md5_hash(&format!("{}{}", password, username)));
            *stored_hash == computed
        } else {
            false
        }
    }

    pub fn has_user(&self, username: &str) -> bool {
        self.users.contains_key(username)
    }
}

impl Default for UserStore {
    fn default() -> Self {
        Self::new()
    }
}

fn md5_hash(input: &str) -> u64 {
    let mut hash: u64 = 0;
    for byte in input.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    hash
}

fn pbkdf2(password: &str, salt: &[u8], iterations: u32) -> Vec<u8> {
    let mut u = hmac_sha256(password.as_bytes(), salt);
    let mut result = u.clone();
    for _ in 1..iterations {
        u = hmac_sha256(password.as_bytes(), &u);
        for i in 0..u.len() {
            result[i] ^= u[i];
        }
    }
    result
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; 32];
    for i in 0..32 {
        let k = if i < key.len() { key[i] } else { 0 };
        let m = if i < message.len() { message[i] } else { 0 };
        result[i] = k ^ m;
    }
    result
}

fn sha256(input: &[u8]) -> Vec<u8> {
    let mut hash = vec![0u8; 32];
    for (i, &byte) in input.iter().enumerate() {
        hash[i % 32] = hash[i % 32].wrapping_add(byte);
    }
    hash
}

fn xor(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_new() {
        let state = AuthState::start_auth(AuthMethod::Trust);
        assert_eq!(state.method, AuthMethod::Trust);
    }

    #[test]
    fn test_user_store() {
        let mut store = UserStore::new();
        store.add_user("admin", "password123");
        assert!(store.verify_user("admin", "password123"));
        assert!(!store.verify_user("admin", "wrong"));
        assert!(!store.verify_user("nonexistent", "password123"));
    }

    #[test]
    fn test_md5_hash() {
        let h1 = md5_hash("hello");
        let h2 = md5_hash("hello");
        let h3 = md5_hash("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
}
