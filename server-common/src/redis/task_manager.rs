// Data Structure in Redis:
//
// 1. Task Hash:
//    - Key: {prefix}:tasks
//    - Type: HSET
//    - Field: {task_id}
//    - Value: task_json (serialized Task object)
//    - TTL: {ttl} seconds
//
// 2. Pending Tasks:
//    - Key: {prefix}:tasks:pending
//    - Type: Set
//    - Members: {task_id}
//    - TTL: {ttl} seconds
//
// 3. Running Tasks:
//   - Key: {prefix}:tasks:running
//   - Type: Set
//   - Members: {task_id}
//   - TTL: {ttl} seconds
//
// 4. Completed Tasks:
//    - Key: {prefix}:tasks:completed
//    - Type: Set
//    - Members: {task_id}
//    - TTL: {ttl} seconds
//
// 5. Results Hash:
//    - Key: {prefix}:results
//    - Type: HSET
//    - Field: {task_id}
//    - Value: result_json (serialized TaskResult object)
//    - TTL: {ttl} seconds
//
// 6. Worker Heartbeats:
//    - Key: {prefix}:heartbeat:{task_id}
//    - Type: String
//    - Value: {worker_id}
//    - TTL: {heartbeat_ttl} seconds

use redis::{aio::Connection, AsyncCommands as _, Client};
use serde::{de::DeserializeOwned, Serialize};
type Result<T> = std::result::Result<T, TaskManagerError>;

#[derive(thiserror::Error, Debug)]
pub enum TaskManagerError {
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("Serde error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

pub struct TaskManager<T: Serialize + DeserializeOwned, R: Serialize + DeserializeOwned> {
    prefix: String,
    ttl: usize,
    heartbeat_ttl: usize,
    client: Client,

    // keys
    tasks_key: String,
    pending_key: String,
    running_key: String,
    completed_key: String,
    results_key: String,
    heartbeat_prefix: String,
    _phantom: std::marker::PhantomData<(T, R)>,
}

impl<T: Serialize + DeserializeOwned, R: Serialize + DeserializeOwned> TaskManager<T, R> {
    pub fn new(
        redis_url: &str,
        prefix: &str,
        ttl: usize,
        heartbeat_ttl: usize,
    ) -> Result<TaskManager<T, R>> {
        let client = Client::open(redis_url)?;
        Ok(TaskManager {
            prefix: prefix.to_owned(),
            ttl,
            heartbeat_ttl,
            client,
            tasks_key: format!("{}:tasks", prefix),
            pending_key: format!("{}:tasks:pending", prefix),
            running_key: format!("{}:tasks:running", prefix),
            completed_key: format!("{}:tasks:completed", prefix),
            results_key: format!("{}:results", prefix),
            heartbeat_prefix: format!("{}:heartbeat", prefix),
            _phantom: std::marker::PhantomData,
        })
    }

    async fn get_connection(&self) -> Result<Connection> {
        Ok(self.client.get_async_connection().await?)
    }

    pub async fn clear_all(&self) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let keys: Vec<String> = conn.keys(format!("{}:*", self.prefix)).await?;
        for key in keys {
            conn.del::<_, ()>(key).await?;
        }
        Ok(())
    }

    pub async fn add_task(&self, task_id: u32, task: &T) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let task_json = serde_json::to_string(task)?;

        let mut pipe = redis::pipe();
        pipe.hset(&self.tasks_key, task_id, task_json.clone())
            .sadd(&self.pending_key, task_id)
            .expire(&self.tasks_key, self.ttl)
            .expire(&self.pending_key, self.ttl);

        pipe.query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    pub async fn check_task_exists(&self, task_id: u32) -> Result<bool> {
        let mut conn = self.get_connection().await?;
        let exists: bool = conn.hexists(&self.tasks_key, task_id).await?;
        Ok(exists)
    }

    pub async fn get_result(&self, task_id: u32) -> Result<Option<R>> {
        let mut conn = self.get_connection().await?;
        let result_json: Option<String> = conn.hget(&self.results_key, task_id).await?;
        if let Some(result_json) = result_json {
            let result: R = serde_json::from_str(&result_json)?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    pub async fn remove_old_tasks(&self, to_task_id: u32) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let task_ids: Vec<u32> = conn.hkeys(&self.tasks_key).await?;
        for task_id in task_ids {
            if task_id <= to_task_id {
                let mut pipe = redis::pipe();
                pipe.srem(&self.pending_key, task_id)
                    .srem(&self.running_key, task_id)
                    .srem(&self.completed_key, task_id)
                    .hdel(&self.tasks_key, task_id)
                    .hdel(&self.results_key, task_id);
                pipe.query_async::<_, ()>(&mut conn).await?;
            }
        }
        Ok(())
    }

    // assign task to worker if available
    pub async fn assign_task(&self) -> Result<Option<(u32, T)>> {
        let mut conn = self.get_connection().await?;

        let script = redis::Script::new(
            r"
            local task_ids = redis.call('SORT', KEYS[1], 'LIMIT', 0, 1)
            if #task_ids == 0 then
                return nil
            end
            local task_id = task_ids[1]
            local task_json = redis.call('HGET', KEYS[3], task_id)
            redis.call('SMOVE', KEYS[1], KEYS[2], task_id)
            redis.call('EXPIRE', KEYS[2], ARGV[1])

            return {task_id, task_json}
        ",
        );

        let result: Option<(u32, String)> = script
            .key(&self.pending_key)
            .key(&self.running_key)
            .key(&self.tasks_key)
            .arg(self.ttl)
            .invoke_async(&mut conn)
            .await?;

        if let Some((task_id, task_json)) = result {
            let task: T = serde_json::from_str(&task_json)?;
            Ok(Some((task_id, task)))
        } else {
            Ok(None)
        }
    }

    pub async fn complete_task(&self, task_id: u32, result: &R) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let result_json = serde_json::to_string(result)?;

        let mut pipe = redis::pipe();

        pipe.hset(&self.results_key, task_id, &result_json)
            .smove(&self.running_key, &self.completed_key, task_id)
            .expire(&self.completed_key, self.ttl)
            .expire(&self.results_key, self.ttl);

        pipe.query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    pub async fn submit_heartbeat(&self, worker_id: &str, task_id: u32) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let key = format!("{}:{}", self.heartbeat_prefix, task_id);
        conn.set_ex::<_, _, ()>(&key, worker_id, self.heartbeat_ttl)
            .await?;
        Ok(())
    }

    pub async fn cleanup_inactive_tasks(&self) -> Result<()> {
        let mut conn = self.get_connection().await?;

        loop {
            // get all running tasks
            let task_ids: Vec<u32> = conn.smembers(&self.running_key).await?;
            log::info!("running tasks: {:?}", task_ids);

            // wait heartbeat_ttl * 3 seconds for worker to submit heartbeat
            tokio::time::sleep(tokio::time::Duration::from_secs(
                (self.heartbeat_ttl * 3) as u64,
            ))
            .await;

            for task_id in task_ids {
                let key = format!("{}:{}", self.heartbeat_prefix, task_id);
                let worker_id: Option<String> = conn.get(&key).await?;
                if worker_id.is_none() {
                    // move task from running to pending
                    conn.smove::<_, _, _, ()>(&self.running_key, &self.pending_key, task_id)
                        .await?;
                    log::warn!("task {} moved from running to pending", task_id);
                }
            }
        }
    }
}
