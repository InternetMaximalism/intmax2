use rand::Rng;

pub fn default_rng() -> impl Rng {
    rand::thread_rng()
}

// #[cfg(not(test))]
// pub fn default_rng() -> impl Rng {
//     rand::rngs::OsRng
// }
