use std::collections::HashMap;
use std::process::Child;

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum JobStatus {
    Running,
    Stopped,
    Done,
}

#[derive(Debug)]
pub struct Job {
    pub id: u32,
    pub pid: u32,
    pub command: String,
    pub status: JobStatus,
    pub process: Option<Child>,
}

pub struct JobManager {
    jobs: HashMap<u32, Job>,
    next_id: u32,
    foreground_pid: Option<u32>,
}

impl JobManager {
    pub fn new() -> Self {
        JobManager {
            jobs: HashMap::new(),
            next_id: 1,
            foreground_pid: None,
        }
    }

    pub fn set_foreground_pid(&mut self, pid: Option<u32>) {
        self.foreground_pid = pid;
    }

    pub fn get_foreground_pid(&self) -> Option<u32> {
        self.foreground_pid
    }

    pub fn add_job(&mut self, pid: u32, command: String, process: Child) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let job = Job {
            id,
            pid,
            command,
            status: JobStatus::Running,
            process: Some(process),
        };

        self.jobs.insert(id, job);
        println!("[{}] {}", id, pid);
        id
    }

    pub fn get_job(&self, id: u32) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn _get_job_mut(&mut self, id: u32) -> Option<&mut Job> {
        self.jobs.get_mut(&id)
    }

    pub fn remove_job(&mut self, id: u32) -> Option<Job> {
        self.jobs.remove(&id)
    }

    pub fn list_jobs(&self) -> Vec<&Job> {
        let mut jobs: Vec<&Job> = self.jobs.values().collect();
        jobs.sort_by_key(|j| j.id);
        jobs
    }

    pub fn update_jobs(&mut self) {
        let mut completed = Vec::new();

        for (id, job) in self.jobs.iter_mut() {
            if let Some(ref mut child) = job.process {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        println!("\n[{}] Done {} (exit: {})", id, job.command, status);
                        job.status = JobStatus::Done;
                        job.process = None;
                        completed.push(*id);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("Error checking job {}: {}", id, e);
                    }
                }
            }
        }

        for id in completed {
            self.jobs.remove(&id);
        }
    }

    pub fn _find_job_by_pid(&self, pid: u32) -> Option<u32> {
        self.jobs.values()
            .find(|j| j.pid == pid)
            .map(|j| j.id)
    }
}
