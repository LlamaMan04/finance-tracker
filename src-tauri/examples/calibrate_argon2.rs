//! Measures the reviewed Argon2id profile on the machine used for security calibration.

use finance_tracker_lib::ports::security::KeyDerivationPort;
use finance_tracker_lib::security::Argon2idKeyDeriver;
use std::time::Instant;

/// Derives one disposable key and reports only parameters and elapsed time.
fn main() {
    let deriver = Argon2idKeyDeriver;
    let metadata = deriver.create_metadata().expect("calibration metadata is generated");
    let started_at = Instant::now();
    let key = deriver
        .derive_database_key(b"disposable calibration passphrase", &metadata)
        .expect("calibration key derives");
    let elapsed = started_at.elapsed();
    drop(key);

    println!(
        "Argon2id v{}: {} KiB, {} passes, {} lane(s), {} ms",
        metadata.kdf_version,
        metadata.argon2.memory_kib,
        metadata.argon2.time_cost,
        metadata.argon2.parallelism,
        elapsed.as_millis()
    );
}
