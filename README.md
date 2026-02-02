# The Move Fuzzer

![Move Fuzzer Demo](demo.gif)

A Move smart‑contract fuzzer targeting both Aptos and Sui dialects. Feedback mechanisms vary by runner (e.g., coverage in Sui stateless, gas‑informed mutation in Aptos stateful).
This project is a fork of [sui-fuzzer](https://github.com/FuzzingLabs/sui-fuzzer), originally developed by [FuzzingLabs](https://fuzzinglabs.com/), and is distributed under the GNU Affero General Public License v3. This fork adds support for Aptos Move and positions the tool as a general Move fuzzer.


## Dependencies

Depending on the target chain, the fuzzer depends on either `aptos-core` or `sui`.

**Aptos**
- `aptos-core` submodule: used as the authoritative source for the Aptos Move framework, VM, executor, and Move tooling (`move-package`, `move-binary-format`, model builder, etc.). Submodule URL: `https://github.com/movementlabsxyz/aptos-core.git` (branch: `l1-migration`).

**Sui**
- `sui` submodule: provides the Sui Move framework, Sui‑specific VM/runtime components, and tooling used by the Sui runners. Submodule URL: `https://github.com/FuzzingLabs/sui.git`.

## Usage

Clone the repository with submodules:

```bash
git clone --recurse-submodules https://github.com/MoveIndustries/move-fuzzer.git
```

#### Build

- Aptos (default feature):  
  ```bash
  ./build-aptos.sh
  ```
- Sui: 
  ```bash
  ./build-sui.sh
  ```

#### Run

- Aptos stateless example:  
  ```bash
  cargo run --release --features aptos -- \
    --config-path ./config_aptos_stateless.json \
    --target-module arithmetic_errors_module \
    --target-function fuzz_u64_overflow
  ```
- Aptos stateful example:  
  ```bash
  cargo run --release --features aptos -- \
    --config-path ./config_aptos_stateful.json \
    --target-module calculator_module \
    --functions add,sub
  ```
- Sui stateless example:  
  ```bash
  cargo run --release --features sui -- \
    --config-path ./config_sui_stateless.json \
    --target-module fuzzinglabs_module \
    --target-function fuzzinglabs
  ```
- Sui stateful example:  
  ```bash
  cargo run --release --features sui -- \
    --config-path ./config_sui_stateful.json \
    --target-module calculator_module \
    --functions add,sub
  ```

Note: in stateful mode, if a `fuzz_init` entry function exists in the target module, it is invoked automatically during each state reset.




#### Configuration

Configuration file (JSON) with a clean example (no comments), followed by field descriptions.

```json
{
  "use_ui": true,
  "nb_threads": 8,
  "seed": 4242,
  "contract": "./examples/fuzzinglabs_package/build/fuzzinglabs_package/bytecode_modules/fuzzinglabs_module.mv",
  "execs_before_cov_update": 10000,
  "execs_before_status_print": 100000,
  "corpus_dir": "./corpus",
  "crashes_dir": "./crashes",
  "fuzz_functions_prefix": "fuzz_",
  "max_call_sequence_size": 5,
  "aptos_helpers": {},
  "aptos_trace_log": null,
  "aptos_stateful_trace_log": null,
  "aptos_build_log": null
}
```

Field descriptions:
- `use_ui`: Enable the TUI.
- `nb_threads`: Number of worker threads.
- `seed`: RNG seed.
- `contract`: Path to the contract. For Sui stateless, this is a compiled `.mv` file. For Aptos (stateless/stateful) and Sui stateful, this is the Move package directory (contains `Move.toml` and `sources/`).
- `execs_before_cov_update`: Frequency of coverage sync.
- `execs_before_status_print`: Status print interval in non-UI mode (optional).
- `corpus_dir`: Output directory for corpus files.
- `crashes_dir`: Output directory for crash files.
- `fuzz_functions_prefix`: Function prefix to list/fuzz.
- `max_call_sequence_size`: Maximum call sequence size (stateful only).
- `aptos_helpers`: Aptos helper mapping (Aptos only).
- `aptos_trace_log`: Path to Aptos stateless trace log (Aptos only, optional).
- `aptos_stateful_trace_log`: Path to Aptos stateful trace log (Aptos only, optional).
- `aptos_build_log`: Path to Aptos package build log (Aptos only, optional).

Feedback notes:
- Sui stateless collects Move VM coverage, maintains a coverage set, and uses it to guide subsequent mutations.
- Aptos stateless does not currently report coverage.
- Aptos stateful uses gas usage as a feedback signal to scale mutation intensity.

## Using Docker

You can also use the provided **docker_run.sh** script to launch it in a container (Sui only). Aptos Docker is not supported.

```bash
$ ./docker_run.sh CONFIG_PATH="./config.json" TARGET_MODULE="fuzzinglabs_module" TARGET_FUNCTION="fuzzinglabs"
```
