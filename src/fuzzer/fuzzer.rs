use bichannel::Channel;
#[cfg(feature = "aptos")]
use aptos_framework::BuiltPackage;
#[cfg(feature = "aptos")]
use aptos_framework::BuildOptions;
#[cfg(feature = "aptos")]
use bcs;
#[cfg(feature = "sui")]
use sui_move_build::BuildConfig;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "aptos")]
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::Instant;
use time::Duration;
use crate::fuzzer::config::Config;
use crate::fuzzer::coverage::Coverage;
use crate::fuzzer::stats::Stats;
use crate::mutator::types::Parameters;
use crate::mutator::types::Type;
use crate::runner::runner::Runner;
use crate::ui::ui::{Ui, UiEvent, UiEventData};
use crate::worker::stateful_worker::StatefulWorker;
use crate::AvailableDetector;
// Sui specific imports
#[cfg(feature = "aptos")]
use crate::mutator::aptos_mutator::AptosMutator;
#[cfg(feature = "sui")]
use crate::mutator::sui_mutator::SuiMutator;
use crate::worker::worker::Worker;
use crate::worker::worker::WorkerEvent;
use crate::worker::stateless_worker::StatelessWorker;
#[cfg(feature = "sui")]
use crate::runner::stateful_runner::sui_runner::SuiRunner as StatefulSuiRunner;
#[cfg(feature = "sui")]
use crate::runner::stateless_runner::sui_runner::SuiRunner as StatelessSuiRunner;
#[cfg(feature = "aptos")]
use crate::runner::stateful_runner::aptos_runner::AptosRunner as StatefulAptosRunner;
#[cfg(feature = "aptos")]
use crate::runner::stateless_runner::aptos_runner::StatelessAptosRunner;
use crate::runner::chain::Chain;
use super::crash::Crash;
#[cfg(feature = "aptos")]
use std::fs::OpenOptions;
use std::io::Write;
use super::fuzzer_utils::load_corpus;
use super::fuzzer_utils::load_crashes;
use super::fuzzer_utils::write_corpusfile;
use super::fuzzer_utils::write_crashfile;
use move_package::compilation::compiled_package::CompiledPackage;
use move_package::compilation::compiled_package::CompiledUnitWithSource;

pub struct Fuzzer {
    // Fuzzer configuration
    config: Config,
    // Thread specific stats
    threads_stats: Vec<Arc<RwLock<Stats>>>,
    // Channel to communicate with each threads
    channels: Vec<Channel<WorkerEvent, WorkerEvent>>,
    // Global stats mostly for ui
    global_stats: Stats,
    // Global coverage
    coverage_set: HashSet<Coverage>,
    // Unique crashes set
    unique_crashes_set: HashSet<Crash>,
    // The user interface
    ui: Option<Ui>,
    // The function to target in the contract
    target_module: String,
    // The function to target in the contract (stateless)
    target_function: Option<String>,
    // The functions to target in the contract (stateful)
    target_functions: Option<Vec<String>>,
    // Parameters of the target function
    target_parameters: Vec<Type>,
    // Max coverage
    max_coverage: usize,
    // Activated detectors
    detectors: Option<Vec<AvailableDetector>>,
    // Whether the fuzzer should use stateful fuzzing or not
    use_state: bool,
}

impl Fuzzer {
    pub fn new_stateless(
        config: Config,
        target_module: &str,
        target_function: &str,
        detectors: Option<&Vec<AvailableDetector>>,
    ) -> Self {
        let nb_threads = config.nb_threads;
        let ui = if config.use_ui {
            Some(Ui::new(nb_threads, config.seed.unwrap()))
        } else {
            None
        };
        let coverage_set = load_corpus(&config.corpus_dir).unwrap_or_default();
        let unique_crashes_set = load_crashes(&config.crashes_dir).unwrap_or_default();
        Fuzzer {
            config,
            threads_stats: vec![],
            channels: vec![],
            global_stats: Stats::new(),
            coverage_set,
            unique_crashes_set,
            ui,
            target_module: String::from(target_module),
            target_function: Some(String::from(target_function)),
            target_functions:None,
            target_parameters: vec![],
            max_coverage: 0,
            detectors: detectors.cloned(),
            use_state: false,
        }
    }

    pub fn new_stateful(
        config: Config,
        target_module: &str,
        target_functions: &Vec<String>,
        detectors: Option<&Vec<AvailableDetector>>,
    ) -> Self {
        let nb_threads = config.nb_threads;
        let ui = if config.use_ui {
            Some(Ui::new(nb_threads, config.seed.unwrap()))
        } else {
            None
        };
        // corpus - A set of interesting test inputs (Not used in the context of sui stateful fuzzer)
        let coverage_set = load_corpus(&config.corpus_dir).unwrap_or_default();
        let unique_crashes_set = load_crashes(&config.crashes_dir).unwrap_or_default();
        Fuzzer {
            config,
            threads_stats: vec![],
            channels: vec![],
            global_stats: Stats::new(),
            coverage_set,
            unique_crashes_set,
            ui,
            target_module: String::from(target_module),
            target_function: None,
            target_functions: Some(target_functions.clone()),
            target_parameters: vec![],
            max_coverage: 0,
            detectors: detectors.cloned(),
            use_state: true,
        }
    }

    fn start_stateless_threads(&mut self) {
        #[cfg(feature = "aptos")]
        let trace_log = self.config.aptos_trace_log.as_ref().map(|path| {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
                .unwrap_or_else(|e| panic!("Unable to open aptos trace log {}: {}", path, e));
            Arc::new(Mutex::new(file))
        });
        for i in 0..self.config.nb_threads {
            // Creates the communication channel for the fuzzer and worker sides
            let (fuzzer, worker) = bichannel::channel::<WorkerEvent, WorkerEvent>();
            self.channels.push(fuzzer);
            let stats = Arc::new(RwLock::new(Stats::new()));
            self.threads_stats.push(stats.clone());
            // Change here the runner you want to create
            if let Some(parameter) = &self.config.contract {
                // Chain is determined at compile time
                #[cfg(feature = "sui")]
                let chain = Chain::Sui;
                #[cfg(feature = "aptos")]
                let chain = Chain::Aptos;
                let seed = self.config.seed.unwrap() + (i as u64);
                let runner: Box<dyn Runner> = match chain {
                    #[cfg(feature = "sui")]
                    Chain::Sui => Box::new(StatelessSuiRunner::new(
                        &parameter.clone(),
                        &self.target_module,
                        &self.target_function.clone().unwrap(),
                    )),
                    #[cfg(feature = "aptos")]
                    Chain::Aptos => {
                        let (metadata, modules) = Self::build_test_modules(
                            self.config.contract.as_ref().unwrap(),
                            self.config.aptos_build_log.as_deref(),
                        );

                        Box::new(StatelessAptosRunner::new(
                            self.config.contract.as_ref().unwrap(),
                            &self.target_module,
                            &self.target_function.clone().unwrap(),
                            metadata,
                            modules,
                            seed,
                            self.config.aptos_helpers.clone(),
                            trace_log.clone(),
                        ))
                    },
                    #[cfg(not(feature = "sui"))]
                    Chain::Sui => unreachable!("Sui feature not enabled"),
                    #[cfg(not(feature = "aptos"))]
                    Chain::Aptos => unreachable!("Aptos feature not enabled"),
                };
                self.target_parameters = runner.get_target_parameters();
                self.max_coverage = runner.get_max_coverage();
                let execs_before_cov_update = self.config.execs_before_cov_update;
                let mutator = match chain {
                    #[cfg(feature = "sui")]
                    Chain::Sui => Box::new(SuiMutator::new(seed, 12)),
                    #[cfg(feature = "aptos")]
                    Chain::Aptos => Box::new(AptosMutator::new(seed, 12)),
                    #[cfg(not(feature = "sui"))]
                    Chain::Sui => unreachable!("Sui feature not enabled"),
                    #[cfg(not(feature = "aptos"))]
                    Chain::Aptos => unreachable!("Aptos feature not enabled"),
                };
                let detectors = self.detectors.clone();
                let coverage_set = self.coverage_set.clone();
                let _ = std::thread::Builder::new()
                    .name(format!("Worker {}", i).to_string())
                    .spawn(move || {
                        // Install panic hook for this thread
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            // Creates generic worker and starts it
                            let mut w = Box::new(StatelessWorker::new(
                                    worker,
                                    stats,
                                    coverage_set,
                                    runner,
                                    mutator,
                                    seed,
                                    execs_before_cov_update,
                                    detectors,
                                ));
                            w.run();
                        }));
                        if let Err(e) = result {
                            eprintln!("Worker thread panicked: {:?}", e);
                        }
                    });
            }
        }
    }

    #[cfg(feature = "sui")]
    fn build_test_modules(test_dir: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend([test_dir]);
        let with_unpublished_deps = false;
        let config = BuildConfig::new_for_testing();
        let package = config.build(path).unwrap();
        (
            package.get_package_digest(with_unpublished_deps).to_vec(),
            package.get_package_bytes(with_unpublished_deps),
        )
    }
#[cfg(feature = "aptos")]
fn build_test_modules(test_dir: &str, build_log: Option<&str>) -> (Vec<u8>, Vec<Vec<u8>>) {
    // Locate to contract source files
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(test_dir);

    log_build(build_log, &format!("Attempting to compile package at: {:?}", path));

        // Check if path exists
    if !path.exists() {
        log_build(build_log, &format!("ERROR: Contract path does not exist: {:?}", path));
        log_build(build_log, "Please check your config file's 'contract' field");
        std::process::exit(1);
    }

    log_build(build_log, "Starting compilation with BuiltPackage...");

        // Use BuiltPackage to compile with proper runtime metadata injection
        // This handles resource_group attributes and other Aptos-specific metadata
        let build_options = BuildOptions {
            dev: false,
            with_srcs: false,
            with_abis: false,
            with_source_maps: false,
            with_error_map: false,
            with_docs: false,
            install_dir: None,
            named_addresses: std::collections::BTreeMap::new(),
            override_std: None,
            docgen_options: None,
            skip_fetch_latest_git_deps: false,
            bytecode_version: None,
            compiler_version: Some(move_model::metadata::CompilerVersion::V2_1),
            language_version: Some(move_model::metadata::LanguageVersion::V2_1),
            skip_attribute_checks: false,
            check_test_code: false,
            known_attributes: aptos_framework::extended_checks::get_all_attribute_names().clone(),
            experiments: vec![],
        };

        let build = || BuiltPackage::build(path.clone(), build_options);
        let built_package = match build_log {
            Some(path) => {
                match redirect_build_output(path, build) {
                    Ok(pkg) => Ok(pkg),
                    Err(e) => Err(e),
                }
            }
            None => build(),
        };

        let built_package = match built_package {
            Ok(pkg) => {
                log_build(build_log, "Compilation successful!");
                pkg
            }
            Err(e) => {
                log_build(build_log, "=== COMPILATION FAILED ===");
                log_build(build_log, &format!("Error: {:?}", e));
                log_build(build_log, "Please fix the compilation errors in your Move package before fuzzing.");
                std::process::exit(1);
            }
        };

        let metadata = bcs::to_bytes(
            &built_package.extract_metadata().expect("extract metadata"),
        )
        .expect("serialize metadata");
        let modules = built_package.extract_code();

    log_build(
        build_log,
        &format!("Extracted {} module(s) with runtime metadata", modules.len()),
    );
    (metadata, modules)
}
    fn start_stateful_threads(&mut self) {

        #[cfg(feature = "sui")]
        let (_, modules) = Self::build_test_modules(self.config.contract.as_ref().unwrap());
        /*
          compiles Move contracts from source code and returns bytecode in modules
         */
        #[cfg(feature = "aptos")]
        let (metadata, modules) = Self::build_test_modules(
            self.config.contract.as_ref().unwrap(),
            self.config.aptos_build_log.as_deref(),
        );

        #[cfg(feature = "aptos")]
        let stateful_trace_log = self.config.aptos_stateful_trace_log.as_ref().map(|path| {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
                .unwrap_or_else(|e| panic!("Unable to open aptos stateful trace log {}: {}", path, e));
            Arc::new(Mutex::new(file))
        });
        #[cfg(not(feature = "aptos"))]
        let stateful_trace_log = None;

        for i in 0..self.config.nb_threads {
            // Creates the communication channel for the fuzzer and worker sides
            let (fuzzer, worker) = bichannel::channel::<WorkerEvent, WorkerEvent>();
            self.channels.push(fuzzer);
            let stats = Arc::new(RwLock::new(Stats::new()));
            self.threads_stats.push(stats.clone());
            // Change here the runner you want to create
            // Chain is determined at compile time
            #[cfg(feature = "sui")]
            let chain = Chain::Sui;
            #[cfg(feature = "aptos")]
            let chain = Chain::Aptos;
            let seed = self.config.seed.unwrap() + (i as u64);
            let runner: Box<dyn crate::runner::runner::StatefulRunner> = match chain {
                #[cfg(feature = "sui")]
                Chain::Sui => Box::new(StatefulSuiRunner::new(
                    &self.target_module,
                    modules.clone(),
                )),
                #[cfg(feature = "aptos")]
                Chain::Aptos => {
                    Box::new(StatefulAptosRunner::new(
                        &self.target_module,
                        metadata.clone(),
                        modules.clone(),
                        seed,
                        self.config.aptos_helpers.clone(),
                    ))
                },
                #[cfg(not(feature = "sui"))]
                Chain::Sui => unreachable!("Sui feature not enabled"),
                #[cfg(not(feature = "aptos"))]
                Chain::Aptos => unreachable!("Aptos feature not enabled"),
            };
            self.max_coverage = runner.get_max_coverage();
            let execs_before_cov_update = self.config.execs_before_cov_update;
            let mutator = match chain {
                #[cfg(feature = "sui")]
                Chain::Sui => Box::new(SuiMutator::new(seed, 12)),
                #[cfg(feature = "aptos")]
                Chain::Aptos => Box::new(AptosMutator::new(seed, 12)),
                #[cfg(not(feature = "sui"))]
                Chain::Sui => unreachable!("Sui feature not enabled"),
                #[cfg(not(feature = "aptos"))]
                Chain::Aptos => unreachable!("Aptos feature not enabled"),
            };
            let detectors = self.detectors.clone();
            let coverage_set = self.coverage_set.clone();
            let target_module = self.target_module.clone();
            let target_functions = self.target_functions.clone().unwrap();
            let fuzz_prefix = self.config.fuzz_functions_prefix .clone();
            let contract = self.config.contract.clone().unwrap();
            let max_call_sequence_size = self.config.max_call_sequence_size;
            let trace_log_for_thread = stateful_trace_log.clone();
            let _ = std::thread::Builder::new()
                .name(format!("Worker {}", i).to_string())
                .spawn(move || {
                    // Creates generic worker and starts it
                    let mut w = Box::new(StatefulWorker::new(
                        &contract,
                        worker,
                        stats,
                        coverage_set,
                        runner,
                        mutator,
                        execs_before_cov_update,
                        seed,
                        detectors,
                        &target_module,
                        target_functions,
                        fuzz_prefix,
                        max_call_sequence_size,
                        trace_log_for_thread,
                    ));
                    w.run();
                });
            }
    }

    fn get_global_execs(&self) -> u64 {
        let mut sum: u64 = 0;
        for i in 0..self.config.nb_threads {
            sum += self.threads_stats[i as usize].read().unwrap().execs;
        }
        sum
    }

    fn get_global_crashes(&self) -> u64 {
        let mut sum: u64 = 0;
        for i in 0..self.config.nb_threads {
            sum += self.threads_stats[i as usize].read().unwrap().crashes;
        }
        sum
    }

    fn broadcast(&self, event: &WorkerEvent) {
        for chan in &self.channels {
            chan.send(event.to_owned()).unwrap();
        }
    }

    fn update_ui(&mut self) {
        if let Some(ui) = &mut self.ui {
            if self.use_state {
                ui.set_target_infos(
                    &self.target_module,
                    &self.target_functions.clone().unwrap().join(", "),
                    &self.target_parameters,
                    self.max_coverage,
                );
            } else {
                ui.set_target_infos(
                    &self.target_module,
                    &self.target_function.clone().unwrap(),
                    &self.target_parameters,
                    self.max_coverage,
                );
            }
        }
    }

    pub fn run(&mut self) {
        // Init workers
        if self.use_state {
            self.start_stateful_threads();
        } else {
            self.start_stateless_threads();
        }

        // Utils for execs per sec
        let mut execs_per_sec_timer = Instant::now();

        let mut events = VecDeque::new();

        let mut new_crash: Option<Crash> = None;

        // Create events for loaded corpus
        for c in self.coverage_set.iter() {
            events.push_front(UiEvent::NewCoverage(UiEventData {
                time: Duration::new(0, 0),
                message: format!("{} - loaded from corpus directory", Parameters(c.inputs.clone())),
                error: None,
            }));
        }

        loop {

            // Update ui infos
            self.update_ui();

            // Sum execs
            self.global_stats.execs = self.get_global_execs();
            self.global_stats.crashes = self.get_global_crashes();

            // Calculate execs_per_sec
            if execs_per_sec_timer.elapsed().as_secs() >= 1 {
                execs_per_sec_timer = Instant::now();
                self.global_stats.execs_per_sec = self.global_stats.execs;
                self.global_stats.time_running += 1;
                self.global_stats.secs_since_last_cov += 1;
                self.global_stats.execs_per_sec =
                    self.global_stats.execs_per_sec / self.global_stats.time_running;
            }

            // Checks channels for new data
            for chan in &self.channels {
                if let Ok(event) = chan.try_recv() {
                    // Creates duration used for the ui
                    let duration =
                        Duration::seconds(self.global_stats.time_running.try_into().unwrap());
                    match event {
                        WorkerEvent::CoverageUpdateRequest(coverage_set) => {
                            // Gets diffrences between the two coverage sets
                            let binding = self.coverage_set.clone();
                            let differences_with_main_thread: HashSet<_> =
                                self.coverage_set.difference(&coverage_set).collect();
                            let differences_with_worker: HashSet<_> =
                                coverage_set.difference(&binding).collect();
                            let mut tmp = HashSet::new();
                            for diff in &differences_with_main_thread.clone() {
                                tmp.insert(diff.to_owned().clone());
                            }
                            // Updates sets
                            if differences_with_main_thread.len() > 0 {
                                chan.send(WorkerEvent::CoverageUpdateResponse(tmp)).unwrap();
                            }
                            // Adds all the coverage to the main coverage_set
                            for diff in &differences_with_worker {
                                if !self.coverage_set.contains(diff) {
                                    write_corpusfile(&self.config.corpus_dir, &diff);
                                    self.coverage_set.insert(diff.to_owned().clone());
                                    self.global_stats.secs_since_last_cov = 0;
                                    self.global_stats.coverage_size += 1;
                                    events.push_front(UiEvent::NewCoverage(UiEventData {
                                        time: duration,
                                        message: format!("{}", Parameters(diff.inputs.clone())),
                                        error: None,
                                    }));
                                }
                            }
                        }
                        WorkerEvent::NewCrash(target_function, inputs, error, call_sequence) => {
                            let mut crash = Crash::new(&self.target_module, &target_function, &inputs, &error);
                            if let Some(seq) = call_sequence {
                                crash = crash.with_call_sequence(seq);
                            }
                            let mut message = format!("{} - already exists, skipping", Parameters(inputs.clone()));
                            if !self.unique_crashes_set.contains(&crash) {
                                write_crashfile(&self.config.crashes_dir, crash.clone());
                                self.global_stats.unique_crashes += 1;
                                self.unique_crashes_set.insert(crash.clone());
                                message = format!("{} - NEW", Parameters(inputs));
                                new_crash = Some(crash);
                            }
                            if self.use_state == false || !message.ends_with("skipping") {
                                events.push_front(UiEvent::NewCrash(UiEventData {
                                    time: duration,
                                    message,
                                    error: Some(error),
                                }));
                            }
                        }
                        WorkerEvent::DetectorTriggered(detector, message) => {
                            let mut final_message = format!("{:?}", detector);
                            if let Some(m) = message {
                                final_message = format!("{:?} -> {}", detector, m);
                            }
                            events.push_front(UiEvent::DetectorTriggered(UiEventData {
                                time: duration,
                                message: final_message,
                                error: None,
                            }));
                        }
                        _ => unimplemented!(),
                    }
                }
            }

            // Broadcasting unique crash to all threads
            if let Some(crash) = &new_crash {
                self.broadcast(&WorkerEvent::NewUniqueCrash(crash.clone()));
                new_crash = None;
            }

            // Run ui
            if self.config.use_ui {
                if self.ui.as_mut().unwrap().render(
                    &self.global_stats,
                    &mut events,
                    &self.threads_stats,
                    &self.detectors,
                    self.use_state
                ) {
                    self.ui.as_mut().unwrap().restore_terminal();
                    eprintln!("Quitting...");
                    break;
                }
            } else {
                for event in events.clone().into_iter() {
                    match event {
                        UiEvent::NewCoverage(data) => println!("New coverage: {}", data.message),
                        UiEvent::NewCrash(data) => {
                            println!("New crash: {} {}", data.error.unwrap(), data.message)
                        }
                        UiEvent::DetectorTriggered(data) => {
                            println!("Detector triggered: {}", data.message)
                        }
                    }
                }
                // Print status every configured exec interval
                let status_interval = self.config.execs_before_status_print;
                if status_interval > 0
                    && self.global_stats.execs > 0
                    && self.global_stats.execs % status_interval == 0
                {
                    println!("{}s running time | {} execs/s | total execs: {} | crashes: {} | unique crashes: {} | coverage: {}", 
                    self.global_stats.time_running, 
                    self.global_stats.execs_per_sec, 
                    self.global_stats.execs, 
                    self.global_stats.crashes, 
                    self.global_stats.unique_crashes, 
                    self.coverage_set.len());
                }
                events.clear();
            }
        }
    }
}

#[cfg(feature = "aptos")]
fn redirect_build_output<F, T>(path: &str, f: F) -> Result<T, anyhow::Error>
where
    F: FnOnce() -> Result<T, anyhow::Error>,
{
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    let file = File::create(path)?;
    let file_fd = file.as_raw_fd();
    unsafe {
        let stdout_fd = libc::dup(libc::STDOUT_FILENO);
        let stderr_fd = libc::dup(libc::STDERR_FILENO);
        if stdout_fd < 0 || stderr_fd < 0 {
            return Err(anyhow::anyhow!("Failed to dup stdout/stderr"));
        }
        if libc::dup2(file_fd, libc::STDOUT_FILENO) < 0 || libc::dup2(file_fd, libc::STDERR_FILENO) < 0 {
            libc::close(stdout_fd);
            libc::close(stderr_fd);
            return Err(anyhow::anyhow!("Failed to redirect stdout/stderr"));
        }
        let result = f();
        libc::dup2(stdout_fd, libc::STDOUT_FILENO);
        libc::dup2(stderr_fd, libc::STDERR_FILENO);
        libc::close(stdout_fd);
        libc::close(stderr_fd);
        result
    }
}

#[cfg(feature = "aptos")]
fn log_build(build_log: Option<&str>, message: &str) {
    if let Some(path) = build_log {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{}", message);
        }
    } else {
        println!("{}", message);
    }
}
