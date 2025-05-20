use super::{error::MerkleTreeError, HashOut, Hasher, MTResult};
use intmax2_zkp::utils::{
    leafable::Leafable,
    leafable_hasher::LeafableHasher,
    trees::{bit_path::BitPath, merkle_tree::MerkleProof},
};

use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Pool, Postgres};

#[derive(Clone, Debug)]
pub struct SqlNodeHashes<V: Leafable + Serialize + DeserializeOwned> {
    tag: u32, // tag is used to distinguish between different trees in the same database
    height: usize,
    zero_hashes: Vec<HashOut<V>>,
    pool: Pool<Postgres>,
}

impl<V: Leafable + Serialize + DeserializeOwned> SqlNodeHashes<V> {
    pub fn new(pool: Pool<Postgres>, tag: u32, height: usize) -> Self {
        let mut zero_hashes = vec![];
        let mut h = V::empty_leaf().hash();
        zero_hashes.push(h);
        for _ in 0..height {
            let new_h = Hasher::<V>::two_to_one(h, h);
            zero_hashes.push(new_h);
            h = new_h;
        }
        zero_hashes.reverse();
        SqlNodeHashes {
            pool,
            tag,
            height,
            zero_hashes,
        }
    }

    pub fn tag(&self) -> u32 {
        self.tag
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub async fn save_node(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        bit_path: BitPath,
        hash: HashOut<V>,
    ) -> MTResult<()> {
        let bit_path = bincode::serialize(&bit_path).unwrap();
        let hash = bincode::serialize(&hash).unwrap();
        sqlx::query!(
            r#"
            INSERT INTO hash_nodes (timestamp_value, tag, bit_path, hash_value)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (timestamp_value, tag, bit_path)
            DO UPDATE SET hash_value = $4
            "#,
            timestamp as i64,
            self.tag as i32,
            bit_path,
            hash,
        )
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }

    async fn get_node_hash(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        bit_path: BitPath,
    ) -> MTResult<HashOut<V>> {
        let bit_path_serialized = bincode::serialize(&bit_path).unwrap();
        let record = sqlx::query!(
            r#"
        SELECT hash_value 
        FROM hash_nodes 
        WHERE bit_path = $1 
          AND timestamp_value <= $2 
          AND tag = $3 
        ORDER BY timestamp_value DESC 
        LIMIT 1
        "#,
            bit_path_serialized,
            timestamp as i64,
            self.tag as i32
        )
        .fetch_optional(tx.as_mut())
        .await?;

        match record {
            Some(row) => {
                let hash = bincode::deserialize(&row.hash_value).unwrap();
                Ok(hash)
            }
            None => {
                let hash = self.zero_hashes[bit_path.len() as usize];
                Ok(hash)
            }
        }
    }

    pub async fn get_sibling_hash(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        path: BitPath,
    ) -> MTResult<HashOut<V>> {
        if path.is_empty() {
            return Err(MerkleTreeError::WrongPathLength(0));
        }
        let sibling_path = path.sibling();
        let sibling_hash = self.get_node_hash(tx, timestamp, sibling_path).await?;
        Ok(sibling_hash)
    }

    pub async fn bulk_get_sibling_hashes(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        paths: &[BitPath],
    ) -> MTResult<Vec<HashOut<V>>> {
        let sibling_paths: Vec<BitPath> = paths.iter().map(|p| p.sibling()).collect();
        let serialized_paths: Vec<Vec<u8>> = sibling_paths
            .iter()
            .map(|p| bincode::serialize(p).unwrap())
            .collect();

        let records = sqlx::query!(
            r#"
            SELECT bit_path, hash_value
            FROM hash_nodes
            WHERE bit_path = ANY($1)
              AND timestamp_value <= $2
              AND tag = $3
            ORDER BY bit_path, timestamp_value DESC
            "#,
            &serialized_paths[..],
            timestamp as i64,
            self.tag as i32
        )
        .fetch_all(tx.as_mut())
        .await?;

        let mut hash_map = std::collections::HashMap::new();
        for record in records {
            let bit_path: BitPath = bincode::deserialize(&record.bit_path).unwrap();
            let hash: HashOut<V> = bincode::deserialize(&record.hash_value).unwrap();
            hash_map.entry(bit_path).or_insert(hash);
        }

        // get results
        let mut results = Vec::with_capacity(sibling_paths.len());
        for sibling_path in sibling_paths {
            let hash = match hash_map.get(&sibling_path) {
                Some(h) => *h,
                None => self.zero_hashes[sibling_path.len() as usize],
            };
            results.push(hash);
        }

        Ok(results)
    }

    pub async fn get_root(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
    ) -> MTResult<HashOut<V>> {
        let root = self
            .get_node_hash(tx, timestamp, BitPath::default())
            .await?;
        Ok(root)
    }

    pub async fn prove(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
        index: u64,
    ) -> MTResult<MerkleProof<V>> {
        let mut path = BitPath::new(self.height as u32, index);
        path.reverse(); // path is big endian
        let mut paths = vec![];
        while !path.is_empty() {
            paths.push(path);
            path.pop();
        }
        let siblings = self.bulk_get_sibling_hashes(tx, timestamp, &paths).await?;

        Ok(MerkleProof { siblings })
    }

    pub async fn reset(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        timestamp: u64,
    ) -> MTResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM hash_nodes
            WHERE tag = $1 AND timestamp_value >= $2
            "#,
            self.tag as i32,
            timestamp as i64
        )
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use intmax2_interfaces::utils::random::default_rng;
    use rand::Rng;
    use sqlx::postgres::PgPoolOptions;

    use super::{BitPath, SqlNodeHashes};
    use intmax2_zkp::utils::leafable::Leafable;

    type TestValue = u32;

    fn setup_test() -> String {
        dotenvy::dotenv().ok();
        std::env::var("DATABASE_URL").unwrap()
    }

    // helper func
    fn collect_paths(path: &BitPath) -> Vec<BitPath> {
        let mut paths = vec![];
        let mut current_path = *path;
        while !current_path.is_empty() {
            paths.push(current_path);
            current_path.pop();
        }
        paths
    }

    #[tokio::test]
    async fn test_prove_empty_tree() -> anyhow::Result<()> {
        let database_url = setup_test();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let mut rng = default_rng();
        let tag = rng.gen();
        let height = 10;
        let node_hashes = SqlNodeHashes::<TestValue>::new(pool.clone(), tag, height);

        let timestamp = 1000;
        let mut tx = pool.begin().await?;

        for index in [0, 1, 5, 10, 100] {
            let proof = node_hashes.prove(&mut tx, timestamp, index).await?;

            let expected_length = height;
            assert_eq!(
                proof.siblings.len(),
                expected_length,
                "Proof for index {} should have {} siblings",
                index,
                expected_length
            );

            let mut path = BitPath::new(height as u32, index);
            path.reverse();
            let paths = collect_paths(&path);

            for (i, sibling) in proof.siblings.iter().enumerate() {
                let sibling_path = paths[i].sibling();
                let expected_zero_hash = node_hashes.zero_hashes[sibling_path.len() as usize];
                assert_eq!(
                    *sibling, expected_zero_hash,
                    "Sibling hash at position {} for index {} should be zero hash",
                    i, index
                );
            }
        }

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_prove_sequential_entries() -> anyhow::Result<()> {
        let database_url = setup_test();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let mut rng = default_rng();
        let tag = rng.gen();
        let height = 10;
        let node_hashes = SqlNodeHashes::<TestValue>::new(pool.clone(), tag, height);

        let timestamp = 1000;
        let mut tx = pool.begin().await?;

        let num_entries = 16;
        let mut expected_hashes = Vec::new();

        for i in 0..num_entries {
            let bit_path = BitPath::new(height as u32, i);
            let hash_value = TestValue::empty_leaf().hash();
            expected_hashes.push((bit_path, hash_value)); // store expected values
            node_hashes
                .save_node(&mut tx, timestamp, bit_path, hash_value)
                .await?;
        }

        for index in 0..num_entries {
            let proof = node_hashes.prove(&mut tx, timestamp, index).await?;

            let mut path = BitPath::new(height as u32, index);
            path.reverse();
            let paths = collect_paths(&path);

            assert_eq!(
                proof.siblings.len(),
                height,
                "Proof for index {} should have {} siblings",
                index,
                height
            );

            for (i, sibling) in proof.siblings.iter().enumerate() {
                let sibling_path = paths[i].sibling();

                let sibling_index = sibling_path.value();
                if sibling_index >= num_entries && i > height - 4 {
                    let expected_zero_hash = node_hashes.zero_hashes[sibling_path.len() as usize];
                    assert_eq!(
                        *sibling, expected_zero_hash,
                        "Sibling hash at position {} for index {} should be zero hash",
                        i, index
                    );
                }
            }
        }

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_prove_sparse_entries() -> anyhow::Result<()> {
        let database_url = setup_test();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let mut rng = default_rng();
        let tag = rng.gen();
        let height = 10;
        let node_hashes = SqlNodeHashes::<TestValue>::new(pool.clone(), tag, height);

        let timestamp = 1000;
        let mut tx = pool.begin().await?;

        let sparse_indices = [100, 200, 500, 1000];
        let hash_value = TestValue::empty_leaf().hash();

        for &i in &sparse_indices {
            let bit_path = BitPath::new(height as u32, i);
            node_hashes
                .save_node(&mut tx, timestamp, bit_path, hash_value)
                .await?;
        }

        for &index in &sparse_indices {
            let proof = node_hashes.prove(&mut tx, timestamp, index).await?;
            let expected_length = height;
            assert_eq!(
                proof.siblings.len(),
                expected_length,
                "Proof for sparse index {} should have {} siblings",
                index,
                expected_length
            );
        }

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_prove_boundary_indices() -> anyhow::Result<()> {
        let database_url = setup_test();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let mut rng = default_rng();
        let tag = rng.gen();
        let height = 10;
        let node_hashes = SqlNodeHashes::<TestValue>::new(pool.clone(), tag, height);

        let timestamp = 1000;
        let mut tx = pool.begin().await?;

        let boundary_indices = [0, (1 << height) - 1, (1 << (height - 1))];
        let hash_value = TestValue::empty_leaf().hash();

        for &i in &boundary_indices {
            let bit_path = BitPath::new(height as u32, i);
            node_hashes
                .save_node(&mut tx, timestamp, bit_path, hash_value)
                .await?;
        }

        for &index in &boundary_indices {
            let proof = node_hashes.prove(&mut tx, timestamp, index).await?;

            let expected_length = height;
            assert_eq!(
                proof.siblings.len(),
                expected_length,
                "Proof for boundary index {} should have {} siblings",
                index,
                expected_length
            );
        }

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_prove_timestamps() -> anyhow::Result<()> {
        let database_url = setup_test();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let mut rng = default_rng();
        let tag = rng.gen();
        let height = 10;
        let node_hashes = SqlNodeHashes::<TestValue>::new(pool.clone(), tag, height);

        let mut tx = pool.begin().await?;

        let timestamps = [500, 800, 1200];
        let test_index = 42;
        let hash_value = TestValue::empty_leaf().hash();

        for &ts in &timestamps {
            let bit_path = BitPath::new(height as u32, test_index);
            node_hashes
                .save_node(&mut tx, ts, bit_path, hash_value)
                .await?;
        }

        for ts in timestamps {
            let proof = node_hashes.prove(&mut tx, ts, test_index).await?;
            let expected_length = height;
            assert_eq!(
                proof.siblings.len(),
                expected_length,
                "Proof at timestamp {} should have {} siblings",
                ts,
                expected_length
            );
        }

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_prove_all_zero_hashes() -> anyhow::Result<()> {
        let database_url = setup_test();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let mut rng = default_rng();
        let tag = rng.gen();
        let height = 10;
        let node_hashes = SqlNodeHashes::<TestValue>::new(pool.clone(), tag, height);

        let timestamp = 1000;
        let mut tx = pool.begin().await?;

        let large_indices = [
            (1 << height) - 2,         // near maximum
            (1 << (height - 1)) + 123, // intermediate
            (1 << height) / 3,         // other value
        ];

        for &index in &large_indices {
            let proof = node_hashes.prove(&mut tx, timestamp, index).await?;

            let mut path = BitPath::new(height as u32, index);
            path.reverse();
            let paths = collect_paths(&path);

            for (i, sibling) in proof.siblings.iter().enumerate() {
                let sibling_path = paths[i].sibling();
                let expected_zero_hash = node_hashes.zero_hashes[sibling_path.len() as usize];
                assert_eq!(
                    *sibling, expected_zero_hash,
                    "Sibling hash at position {} for index {} should be zero hash",
                    i, index
                );
            }
        }

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_prove_mixed_zero_and_real_hashes() -> anyhow::Result<()> {
        let database_url = setup_test();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let mut rng = default_rng();
        let tag = rng.gen();
        let height = 10;
        let node_hashes = SqlNodeHashes::<TestValue>::new(pool.clone(), tag, height);

        let timestamp = 1000;
        let mut tx = pool.begin().await?;

        let test_index = 42;
        let mut path = BitPath::new(height as u32, test_index);
        path.reverse();

        let paths = collect_paths(&path);

        for i in (0..paths.len()).step_by(2) {
            let path_to_save = paths[i];
            let sibling_path = path_to_save.sibling();

            let custom_hash = TestValue::empty_leaf().hash();
            node_hashes
                .save_node(&mut tx, timestamp, sibling_path, custom_hash)
                .await?;
        }

        let proof = node_hashes.prove(&mut tx, timestamp, test_index).await?;

        for (i, sibling) in proof.siblings.iter().enumerate() {
            let sibling_path = paths[i].sibling();

            if i % 2 == 0 {
                let saved_hash = TestValue::empty_leaf().hash();
                assert_eq!(
                    *sibling, saved_hash,
                    "Sibling hash at position {} should be custom hash",
                    i
                );
            } else {
                let expected_zero_hash = node_hashes.zero_hashes[sibling_path.len() as usize];
                assert_eq!(
                    *sibling, expected_zero_hash,
                    "Sibling hash at position {} should be zero hash",
                    i
                );
            }
        }

        tx.rollback().await?;
        Ok(())
    }
}
