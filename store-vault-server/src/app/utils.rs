use uuid::Uuid;

pub fn extract_timestamp_from_uuidv7(uuid: &Uuid) -> (u64, u64) {
    let bytes = uuid.as_bytes();
    let ts_ms = ((bytes[0] as u64) << 40)
        | ((bytes[1] as u64) << 32)
        | ((bytes[2] as u64) << 24)
        | ((bytes[3] as u64) << 16)
        | ((bytes[4] as u64) << 8)
        | (bytes[5] as u64);
    let ts_s = ts_ms / 1000;
    (ts_s, ts_ms)
}
