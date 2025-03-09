use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;

#[async_trait::async_trait(?Send)]
pub trait AsyncTask {
    type Output: Send + Clone;
    type Error: std::error::Error + Send + Sync;

    async fn execute(id: usize) -> Result<Self::Output, Self::Error>;
}

struct TaskQueue<T: AsyncTask> {
    tasks: Vec<usize>,
    completed: usize,
    results: Vec<T::Output>,
    errors: Vec<String>,
}

impl<T: AsyncTask> TaskQueue<T> {
    fn new(total_tasks: usize) -> Self {
        let mut tasks = Vec::with_capacity(total_tasks);
        for i in 0..total_tasks {
            tasks.push(i);
        }

        Self {
            tasks,
            completed: 0,
            results: Vec::new(),
            errors: Vec::new(),
        }
    }
}

pub async fn process_queue<T: AsyncTask + 'static>(
    total_tasks: usize,
    max_concurrent: usize,
) -> Vec<T::Output> {
    #[cfg(feature = "failpoints")]
    assert_eq!(max_concurrent, 1, "When the failpoints feature is enabled, please set the maximum degree of parallelism to 1.");

    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let queue = Arc::new(Mutex::new(TaskQueue::<T>::new(total_tasks)));
    let queue_clone = queue.clone();

    let local = tokio::task::LocalSet::new();
    for worker_id in 0..max_concurrent {
        let worker_semaphore = semaphore.clone();
        let worker_queue = queue_clone.clone();

        local.spawn_local(async move {
            loop {
                let task_id = {
                    let mut queue = worker_queue.lock().unwrap();
                    if queue.tasks.is_empty() {
                        break;
                    }
                    queue.tasks.remove(0)
                };

                let permit = worker_semaphore.acquire().await.unwrap();
                log::info!("Worker {} picked task {}", worker_id, task_id);

                let result = T::execute(task_id).await;

                {
                    let mut queue = worker_queue.lock().unwrap();
                    queue.completed += 1;

                    match result {
                        Ok(success) => queue.results.push(success),
                        Err(error) => queue.errors.push(error.to_string()),
                    }

                    let progress = queue.completed as f64 / total_tasks as f64 * 100.0;
                    log::info!("Progress: {:.1}%", progress);
                }

                drop(permit);

                let mut queue = worker_queue.lock().unwrap();
                let last_index = queue.tasks.len();
                queue.tasks.insert(last_index, task_id);
            }
        });
    }

    local.spawn_local(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;

            let mut queue = queue_clone.lock().unwrap();
            let errors = queue.errors.clone();
            if !errors.is_empty() {
                log::error!("\nErrors:");
                for (i, error) in errors.iter().enumerate() {
                    log::error!("  {}. {}", i + 1, error);
                }
            }

            // Reset errors
            queue.errors.clear();
        }
    });

    local.await;

    let queue = queue.lock().unwrap();

    queue.results.clone()
}
