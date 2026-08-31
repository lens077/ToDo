use std::collections::HashSet;

use getrandom::fill;

use crate::{Error, Result};

pub const DEFAULT_ID_LENGTH: usize = 6;

/// Generates a random base36 ID absent from `existing`.
///
/// Randomness deliberately avoids time-adjacent IDs across concurrent git branches, so
/// human reviewers can distinguish independently-created task lines. On collision the
/// candidate grows by one character rather than retrying forever in the same namespace.
pub fn generate_task_id(existing: &HashSet<String>) -> Result<String> {
    generate_task_id_with(existing, |bytes| {
        fill(bytes).map_err(|error| Error::RandomSource(error.to_string()))
    })
}

fn generate_task_id_with<F>(existing: &HashSet<String>, mut random: F) -> Result<String>
where
    F: FnMut(&mut [u8]) -> Result<()>,
{
    let mut length = DEFAULT_ID_LENGTH;
    loop {
        let candidate = random_base36(length, &mut random)?;
        if !existing.contains(&candidate) {
            return Ok(candidate);
        }
        length += 1;
    }
}

fn random_base36<F>(length: usize, random: &mut F) -> Result<String>
where
    F: FnMut(&mut [u8]) -> Result<()>,
{
    let mut output = String::with_capacity(length);
    while output.len() < length {
        let mut bytes = [0u8; 32];
        random(&mut bytes)?;
        for byte in bytes {
            // Rejection sampling avoids modulo bias: 252 is divisible by 36.
            if byte < 252 {
                let digit = byte % 36;
                output.push(if digit < 10 {
                    char::from(b'0' + digit)
                } else {
                    char::from(b'a' + digit - 10)
                });
                if output.len() == length {
                    break;
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_extends_candidate_length() {
        let existing = HashSet::from(["000000".to_string()]);
        let mut calls = 0;
        let id = generate_task_id_with(&existing, |bytes| {
            calls += 1;
            bytes.fill(if calls == 1 { 0 } else { 1 });
            Ok(())
        })
        .unwrap();
        assert_eq!(id, "1111111");
        assert_eq!(id.len(), DEFAULT_ID_LENGTH + 1);
    }

    #[test]
    fn thousand_ids_are_unique_and_prefixes_are_dispersed() {
        let mut existing = HashSet::new();
        let mut prefixes = HashSet::new();
        for _ in 0..1000 {
            let id = generate_task_id(&existing).unwrap();
            assert_eq!(id.len(), DEFAULT_ID_LENGTH);
            assert!(id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase()));
            prefixes.insert(id[..2].to_string());
            assert!(existing.insert(id));
        }
        // 1000 samples over 1296 buckets normally occupy ~697 buckets. A threshold of
        // 300 is astronomically below random expectation but decisively rejects a
        // timestamp sequence whose first two characters remain nearly constant.
        assert!(
            prefixes.len() > 300,
            "only {} distinct prefixes",
            prefixes.len()
        );
    }
}
