use py_spy::{config::Config, sampler::Sampler};

use pyroscope::{
    backend::{Backend, Report, StackFrame, StackTrace, State},
    error::{PyroscopeError, Result},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

/// Pyspy Configuration
#[derive(Debug, Clone)]
pub struct PyspyConfig {
    /// Process to monitor
    pid: Option<i32>,
    /// Sampling rate
    sample_rate: u32,
    /// Lock Process while sampling
    lock_process: py_spy::config::LockingStrategy,
    /// Profiling duration. None for infinite.
    time_limit: Option<core::time::Duration>,
    /// Include subprocesses
    with_subprocesses: bool,
    /// todo
    include_idle: bool,
    /// todo
    gil_only: bool,
    /// todo
    native: bool,
}

impl Default for PyspyConfig {
    fn default() -> Self {
        PyspyConfig {
            pid: Some(0),
            sample_rate: 100,
            lock_process: py_spy::config::LockingStrategy::NonBlocking,
            time_limit: None,
            with_subprocesses: false,
            include_idle: false,
            gil_only: false,
            native: false,
        }
    }
}

impl PyspyConfig {
    /// Create a new PyspyConfig
    pub fn new(pid: i32) -> Self {
        PyspyConfig {
            pid: Some(pid),
            ..Default::default()
        }
    }

    /// Set the sampling rate
    pub fn sample_rate(self, sample_rate: u32) -> Self {
        PyspyConfig {
            sample_rate,
            ..self
        }
    }

    /// Set the lock process flag
    pub fn lock_process(self, lock_process: bool) -> Self {
        PyspyConfig {
            lock_process: if lock_process {
                py_spy::config::LockingStrategy::Lock
            } else {
                py_spy::config::LockingStrategy::NonBlocking
            },
            ..self
        }
    }

    /// Set the time limit
    pub fn time_limit(self, time_limit: Option<core::time::Duration>) -> Self {
        PyspyConfig { time_limit, ..self }
    }

    /// Include subprocesses
    pub fn with_subprocesses(self, with_subprocesses: bool) -> Self {
        PyspyConfig {
            with_subprocesses,
            ..self
        }
    }

    /// Include idle time
    pub fn include_idle(self, include_idle: bool) -> Self {
        PyspyConfig {
            include_idle,
            ..self
        }
    }

    pub fn gil_only(self, gil_only: bool) -> Self {
        PyspyConfig { gil_only, ..self }
    }

    pub fn native(self, native: bool) -> Self {
        PyspyConfig { native, ..self }
    }
}

/// Pyspy Backend
#[derive(Default)]
pub struct Pyspy {
    /// Pyspy State
    state: State,
    /// Profiling buffer
    buffer: Arc<Mutex<Report>>,
    /// Pyspy Configuration
    config: PyspyConfig,
    /// Sampler configuration
    sampler_config: Option<Config>,
    /// Sampler thread
    sampler_thread: Option<JoinHandle<Result<()>>>,
    /// Atomic flag to stop the sampler
    running: Arc<AtomicBool>,
}

impl std::fmt::Debug for Pyspy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pyspy Backend")
    }
}

impl Pyspy {
    /// Create a new Pyspy Backend
    pub fn new(config: PyspyConfig) -> Self {
        Pyspy {
            state: State::Uninitialized,
            buffer: Arc::new(Mutex::new(Report::default())),
            config,
            sampler_config: None,
            sampler_thread: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Backend for Pyspy {
    fn get_state(&self) -> State {
        self.state
    }

    fn spy_name(&self) -> Result<String> {
        Ok("pyspy".to_string())
    }

    fn sample_rate(&self) -> Result<u32> {
        Ok(self.config.sample_rate)
    }

    fn initialize(&mut self) -> Result<()> {
        // Check if Backend is Uninitialized
        if self.state != State::Uninitialized {
            return Err(PyroscopeError::new("Rbspy: Backend is already Initialized"));
        }

        // Check if a process ID is set
        if self.config.pid.is_none() {
            return Err(PyroscopeError::new("Rbspy: No Process ID Specified"));
        }

        // Set duration for py-spy
        let duration = match self.config.time_limit {
            Some(duration) => py_spy::config::RecordDuration::Seconds(duration.as_secs()),
            None => py_spy::config::RecordDuration::Unlimited,
        };

        // Create a new py-spy configuration
        self.sampler_config = Some(Config {
            blocking: self.config.lock_process.clone(),
            native: self.config.native,
            pid: self.config.pid,
            sampling_rate: self.config.sample_rate as u64,
            include_idle: self.config.include_idle,
            include_thread_ids: true,
            subprocesses: self.config.with_subprocesses,
            gil_only: self.config.gil_only,
            duration,
            ..Config::default()
        });

        // Set State to Ready
        self.state = State::Ready;

        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        // Check if Backend is Ready
        if self.state != State::Ready {
            return Err(PyroscopeError::new("Rbspy: Backend is not Ready"));
        }

        let running = Arc::clone(&self.running);
        // set running to true
        running.store(true, Ordering::Relaxed);

        let buffer = self.buffer.clone();

        let config = self.sampler_config.clone().unwrap();

        self.sampler_thread = Some(std::thread::spawn(move || {
            let sampler = Sampler::new(config.pid.unwrap(), &config)
                .map_err(|e| PyroscopeError::new(&format!("Rbspy: Sampler Error: {}", e)))?;

            let isampler = sampler.take_while(|_x| running.load(Ordering::Relaxed));

            for sample in isampler {
                for trace in sample.traces {
                    if !(config.include_idle || trace.active) {
                        continue;
                    }

                    if config.gil_only && !trace.owns_gil {
                        continue;
                    }

                    let own_trace: StackTrace =
                        Into::<StackTraceWrapper>::into(trace.clone()).into();

                    buffer.lock()?.record(own_trace)?;
                }
            }

            Ok(())
        }));

        // Set State to Running
        self.state = State::Running;

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // Check if Backend is Running
        if self.state != State::Running {
            return Err(PyroscopeError::new("Rbspy: Backend is not Running"));
        }

        // set running to false
        self.running.store(false, Ordering::Relaxed);

        // wait for sampler thread to finish
        self.sampler_thread.take().unwrap().join().unwrap()?;

        // Set State to Running
        self.state = State::Ready;

        Ok(())
    }

    fn report(&mut self) -> Result<Vec<u8>> {
        // Check if Backend is Running
        if self.state != State::Running {
            return Err(PyroscopeError::new("Rbspy: Backend is not Running"));
        }

        let buffer = self.buffer.clone();

        let v8: Vec<u8> = buffer.lock()?.to_string().into_bytes();

        buffer.lock()?.clear();

        Ok(v8)
    }
}
struct StackFrameWrapper(StackFrame);

impl From<StackFrameWrapper> for StackFrame {
    fn from(stack_frame: StackFrameWrapper) -> Self {
        stack_frame.0
    }
}

impl From<py_spy::Frame> for StackFrameWrapper {
    fn from(frame: py_spy::Frame) -> Self {
        StackFrameWrapper(StackFrame {
            module: frame.module.clone(),
            name: Some(frame.name.clone()),
            filename: frame.short_filename.clone(),
            relative_path: None,
            absolute_path: Some(frame.filename.clone()),
            line: Some(frame.line as u32),
        })
    }
}

struct StackTraceWrapper(StackTrace);

impl From<StackTraceWrapper> for StackTrace {
    fn from(stack_trace: StackTraceWrapper) -> Self {
        stack_trace.0
    }
}

impl From<py_spy::StackTrace> for StackTraceWrapper {
    fn from(trace: py_spy::StackTrace) -> Self {
        StackTraceWrapper(StackTrace {
            pid: Some(trace.pid as u32),
            thread_id: Some(trace.thread_id as u64),
            thread_name: trace.thread_name.clone(),
            frames: trace
                .frames
                .iter()
                .map(|frame| Into::<StackFrameWrapper>::into(frame.clone()).into())
                .collect(),
        })
    }
}
