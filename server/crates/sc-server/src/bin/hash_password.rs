use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher,
};

fn main() {
    let password = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "admin".to_string());
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();
    println!("{}", hash);
}
