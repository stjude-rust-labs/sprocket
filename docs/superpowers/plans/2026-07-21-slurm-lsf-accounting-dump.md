# Slurm/LSF Job Accounting Dump Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After a Slurm or LSF job finishes (successfully or not), gather accounting information via `sacct`/`bjobs` and write it as JSON into the task's attempt directory, opt-out via config.

**Architecture:** Each backend gets one new best-effort, retried call to `sacct`/`bjobs` at the point its existing completion oneshot resolves (not on user-initiated cancellation), writing a JSON file next to the task's existing `stdout`/`stderr`/`command` files. A new `job_accounting: Option<bool>` config field (default `true`) gates the whole thing off per backend.

**Tech Stack:** Rust, `tokio-retry2` (already a dependency, already used the same way in `backend/apptainer.rs`), `serde_json` (already a dependency).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-21-slurm-lsf-accounting-dump-design.md` — read it before starting; this plan implements it directly.
- The accounting dump must never change a task's exit code or turn a successful/failed result into a different outcome — all failures in this new code path are logged (`warn!`) and swallowed.
- Only gathered on the job's normal completion-oneshot arm (`slurm_apptainer.rs:1085`, `lsf_apptainer.rs:979`), never on the cancellation branches.
- No new dependencies — `tokio-retry2` and `serde_json` are already direct dependencies of the `wdl-engine` crate (`crates/wdl-engine/Cargo.toml:40-41,48`).
- Follow the existing `Retry::spawn_notify` + `ExponentialBackoff` + `RetryError::transient`/`RetryError::permanent` pattern from `crates/wdl-engine/src/backend/apptainer.rs:327-345,402-465` — do not write a bespoke retry loop.
- Run `cargo --locked fmt -- --check` and `cargo --locked clippy --workspace --tests --all-features -- --deny warnings` before considering any task done (matches `.github/workflows/CI.yml:37,65`).

---

## File Structure

- Modify: `crates/wdl-engine/src/config.rs` — add `job_accounting: Option<bool>` to `SlurmApptainerBackendConfig` and `LsfApptainerBackendConfig`.
- Modify: `jsonschemas/sprocket.toml.json` — regenerated, not hand-edited.
- Modify: `crates/wdl-engine/src/backend/slurm_apptainer.rs` — new consts, `Monitor::read_job_accounting`, `Monitor::write_job_accounting`, `parse_accounting_output`, `accounting_output_is_empty`, a new `#[cfg(test)] mod tests`, and one gated call added to `execute()`.
- Modify: `crates/wdl-engine/src/backend/lsf_apptainer.rs` — same shape: new consts, `Monitor::read_job_accounting`, `Monitor::write_job_accounting`, `parse_accounting_output`, new tests added to the existing `#[cfg(test)] mod tests` (`lsf_apptainer.rs:1041-1052`), and one gated call added to `execute()`.
- Modify: `crates/wdl-engine/CHANGELOG.md` — one `Unreleased` entry covering both backends.

---

### Task 1: Add `job_accounting` config option to both backends

**Files:**
- Modify: `crates/wdl-engine/src/config.rs:2545-2561` (`SlurmApptainerBackendConfig`)
- Modify: `crates/wdl-engine/src/config.rs:2317-2330` (`LsfApptainerBackendConfig`)
- Modify: `jsonschemas/sprocket.toml.json` (regenerated)
- Test: `src/config.rs:1111-1140` (existing `public_schema_up_to_date` / `schema_matches_config` tests — no new test code needed, just must keep passing)

**Interfaces:**
- Produces: `SlurmApptainerBackendConfig.job_accounting: Option<bool>` and `LsfApptainerBackendConfig.job_accounting: Option<bool>`, both read via `.unwrap_or(true)` at the call site (Tasks 3 and 5 consume this).

- [ ] **Step 1: Add the field to `SlurmApptainerBackendConfig`**

In `crates/wdl-engine/src/config.rs`, find:

```rust
    /// The maximum number of concurrent Slurm operations the backend will
    /// perform.
    ///
    /// This controls the maximum concurrent number of `sbatch` processes the
    /// backend will spawn to queue tasks.
    ///
    /// Defaults to 10 concurrent operations.
    pub max_concurrency: Option<u32>,
    /// Which partition, if any, to specify when submitting normal jobs to
    /// Slurm.
```

Insert a new field directly after `max_concurrency` and before the `default_slurm_partition` doc comment:

```rust
    /// The maximum number of concurrent Slurm operations the backend will
    /// perform.
    ///
    /// This controls the maximum concurrent number of `sbatch` processes the
    /// backend will spawn to queue tasks.
    ///
    /// Defaults to 10 concurrent operations.
    pub max_concurrency: Option<u32>,
    /// Whether to gather Slurm accounting information for a task's job via
    /// `sacct` once the job reaches a terminal state, writing it to
    /// `sacct.json` in the task's attempt directory.
    ///
    /// This is best-effort: failures gathering this information are logged
    /// but never affect the task's own result.
    ///
    /// Defaults to `true`.
    pub job_accounting: Option<bool>,
    /// Which partition, if any, to specify when submitting normal jobs to
    /// Slurm.
```

- [ ] **Step 2: Add the same field to `LsfApptainerBackendConfig`**

In the same file, find:

```rust
    /// The maximum number of concurrent LSF operations the backend will
    /// perform.
    ///
    /// This controls the maximum concurrent number of `bsub` processes the
    /// backend will spawn to queue tasks.
    ///
    /// Defaults to 10 concurrent operations.
    pub max_concurrency: Option<u32>,
    /// Which queue, if any, to specify when submitting normal jobs to LSF.
```

Insert:

```rust
    /// The maximum number of concurrent LSF operations the backend will
    /// perform.
    ///
    /// This controls the maximum concurrent number of `bsub` processes the
    /// backend will spawn to queue tasks.
    ///
    /// Defaults to 10 concurrent operations.
    pub max_concurrency: Option<u32>,
    /// Whether to gather LSF accounting information for a task's job via
    /// `bjobs` once the job reaches a terminal state, writing it to
    /// `bjobs.json` in the task's attempt directory.
    ///
    /// This is best-effort: failures gathering this information are logged
    /// but never affect the task's own result.
    ///
    /// Defaults to `true`.
    pub job_accounting: Option<bool>,
    /// Which queue, if any, to specify when submitting normal jobs to LSF.
```

- [ ] **Step 3: Regenerate the JSON schema**

Run:

```bash
cargo run --quiet -- config schema > jsonschemas/sprocket.toml.json
```

- [ ] **Step 4: Verify the schema and config tests pass**

Run: `cargo test -p sprocket --lib config::test`
Expected: `public_schema_up_to_date` and `schema_matches_config` both PASS (they will fail with a diff if step 3 was skipped or the field text doesn't match).

- [ ] **Step 5: Verify the crate still builds**

Run: `cargo check -p wdl-engine -p sprocket`
Expected: no errors (the field is unused so far — that's fine, it's a public struct field, not a local variable, so there's no dead-code warning).

- [ ] **Step 6: Commit**

```bash
git add crates/wdl-engine/src/config.rs jsonschemas/sprocket.toml.json
git commit -m "feat(config): add job_accounting option to slurm/lsf backends"
```

---

### Task 2: Slurm accounting fetch/parse (pure functions + tests)

**Files:**
- Modify: `crates/wdl-engine/src/backend/slurm_apptainer.rs`
- Test: same file, new `#[cfg(test)] mod tests` block at the end

**Interfaces:**
- Consumes: nothing new from other tasks.
- Produces: `Monitor::read_job_accounting(job_id: u64) -> Result<Vec<u8>>` (async), `parse_accounting_output(output: &[u8]) -> Result<Vec<serde_json::Value>>`, `accounting_output_is_empty(output: &[u8]) -> bool`, `ACCOUNTING_FIELDS: &str`, `ACCOUNTING_FILE_NAME: &str`. Task 3 calls `Monitor::read_job_accounting` and `parse_accounting_output`.

- [ ] **Step 1: Add the new imports**

In `crates/wdl-engine/src/backend/slurm_apptainer.rs`, find:

```rust
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
```

Replace with:

```rust
use tokio::time::MissedTickBehavior;
use tokio_retry2::Retry;
use tokio_retry2::RetryError;
use tokio_retry2::strategy::ExponentialBackoff;
use tokio_util::sync::CancellationToken;
```

- [ ] **Step 2: Add the new constants**

Find:

```rust
/// The default maximum concurrency for `sbatch` and `scancel` operations.
const DEFAULT_MAX_CONCURRENCY: u32 = 10;
```

Insert directly after it:

```rust
/// The default maximum concurrency for `sbatch` and `scancel` operations.
const DEFAULT_MAX_CONCURRENCY: u32 = 10;

/// The name of the file where a job's final accounting information (from
/// `sacct`) is written.
const ACCOUNTING_FILE_NAME: &str = "sacct.json";

/// The fields requested from `sacct` when gathering final accounting
/// information for a single terminated job.
///
/// This is a superset of [`JobRecord::fields`], since this query is only
/// made once per job (on termination) rather than on every monitor tick for
/// all currently-tracked jobs at once. Job-step lines (e.g. `.batch`,
/// `.extern`) are intentionally not filtered out here, unlike in
/// [`MonitorState::update_jobs`]: Slurm frequently only reports memory/IO
/// statistics on those step lines rather than the parent job line.
const ACCOUNTING_FIELDS: &str = "JobID,JobName,Partition,State,ExitCode,NodeList,Submit,Start,\
     End,Elapsed,AllocCPUS,ReqMem,ReqTRES,AllocTRES,MaxRSS,MaxVMSize,AveRSS,AveVMSize,TotalCPU,\
     UserCPU,SystemCPU,MaxDiskRead,MaxDiskWrite";

/// The initial delay, in milliseconds, before retrying a failed or
/// incomplete accounting query.
const ACCOUNTING_RETRY_INITIAL_DELAY_MS: u64 = 500;

/// The maximum delay, in milliseconds, between accounting query retries.
const ACCOUNTING_RETRY_MAX_DELAY_MS: u64 = 5_000;

/// The maximum number of attempts made to gather a job's accounting
/// information before giving up.
const ACCOUNTING_RETRY_ATTEMPTS: usize = 5;
```

- [ ] **Step 3: Add `parse_accounting_output` and `accounting_output_is_empty`**

Find the end of the `impl Monitor` block:

```rust
        Ok(output.stdout)
    }
}

/// Represents a submitted Slurm job.
```

Replace with (adding two free functions between the closing `}` of `impl Monitor` and the `SubmittedJob` struct — note `SubmittedJob` is defined earlier in the file than `impl Monitor`, so in the actual file this insertion point is the `}` that closes `impl Monitor` at line 766, immediately before the `SlurmApptainerBackend` struct doc comment):

```rust
        Ok(output.stdout)
    }
}

/// Parses the pipe-delimited output of an accounting `sacct` query (using
/// [`ACCOUNTING_FIELDS`]) into one JSON object per line returned — i.e., the
/// job itself plus any job steps — keyed by field name. Values are kept as
/// the raw strings `sacct` emits; no unit or duration parsing is performed.
fn parse_accounting_output(output: &[u8]) -> Result<Vec<serde_json::Value>> {
    let output = str::from_utf8(output).context("`sacct` output was not UTF-8")?;

    Ok(output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: serde_json::Map<String, serde_json::Value> = ACCOUNTING_FIELDS
                .split(',')
                .zip(line.split('|'))
                .map(|(name, value)| {
                    (name.to_string(), serde_json::Value::String(value.to_string()))
                })
                .collect();
            serde_json::Value::Object(fields)
        })
        .collect())
}

/// Returns `true` if the output of an accounting `sacct` query contains no
/// records, which signals that Slurm's accounting database (`slurmdbd`)
/// hasn't yet caught up with the job's termination.
fn accounting_output_is_empty(output: &[u8]) -> bool {
    output.iter().all(u8::is_ascii_whitespace)
}
```

- [ ] **Step 4: Add `Monitor::read_job_accounting`**

Find (this is the same `read_jobs` function whose end you just edited around in Step 3 — add this new method directly before it, still inside `impl Monitor`):

```rust
    /// Reads the current jobs using `sacct`.
    ///
    /// Returns the stdout of `sacct`.
    async fn read_jobs(jobs: &str) -> Result<Vec<u8>> {
```

Replace with:

```rust
    /// Reads final accounting information for a single terminated job using
    /// `sacct`, retrying briefly since Slurm's accounting database can lag
    /// behind job termination.
    ///
    /// Returns the raw (pipe-delimited) stdout of `sacct`.
    async fn read_job_accounting(job_id: u64) -> Result<Vec<u8>> {
        async fn try_read(job_id: u64) -> Result<Vec<u8>, RetryError<anyhow::Error>> {
            let mut command = Command::new("sacct");
            let command = command
                .arg("-P")
                .arg("-n")
                .arg("--format")
                .arg(ACCOUNTING_FIELDS)
                .arg("-j")
                .arg(job_id.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            trace!(?command, "spawning `sacct` to gather job accounting information");

            let child = command
                .spawn()
                .context("failed to spawn `sacct` command")
                .map_err(RetryError::transient)?;

            let output = child
                .wait_with_output()
                .await
                .context("failed to wait for `sacct` to exit")
                .map_err(RetryError::transient)?;
            if !output.status.success() {
                return Err(RetryError::transient(anyhow!(
                    "`sacct` failed: {status}: {stderr}",
                    status = output.status,
                    stderr = str::from_utf8(&output.stderr)
                        .unwrap_or("<output not UTF-8>")
                        .trim()
                )));
            }

            if accounting_output_is_empty(&output.stdout) {
                return Err(RetryError::transient(anyhow!(
                    "`sacct` returned no accounting records for Slurm job `{job_id}`"
                )));
            }

            Ok(output.stdout)
        }

        Retry::spawn_notify(
            ExponentialBackoff::from_millis(ACCOUNTING_RETRY_INITIAL_DELAY_MS)
                .max_delay_millis(ACCOUNTING_RETRY_MAX_DELAY_MS)
                .take(ACCOUNTING_RETRY_ATTEMPTS),
            || try_read(job_id),
            |e: &anyhow::Error, _| {
                warn!(e = %e, "retrying `sacct` accounting query for Slurm job `{job_id}`");
            },
        )
        .await
    }

    /// Reads the current jobs using `sacct`.
    ///
    /// Returns the stdout of `sacct`.
    async fn read_jobs(jobs: &str) -> Result<Vec<u8>> {
```

- [ ] **Step 5: Write the failing tests**

Add at the end of `crates/wdl-engine/src/backend/slurm_apptainer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a single `sacct`-style pipe-delimited line for [`ACCOUNTING_FIELDS`],
    /// with all fields empty except the ones given in `overrides`.
    fn accounting_line(overrides: &[(&str, &str)]) -> String {
        let names: Vec<&str> = ACCOUNTING_FIELDS.split(',').collect();
        let mut values = vec![String::new(); names.len()];
        for (name, value) in overrides {
            let idx = names
                .iter()
                .position(|n| n == name)
                .unwrap_or_else(|| panic!("unknown accounting field `{name}`"));
            values[idx] = (*value).to_string();
        }
        values.join("|")
    }

    #[test]
    fn parses_accounting_output_into_one_record_per_line() {
        let job_line = accounting_line(&[
            ("JobID", "12345"),
            ("State", "COMPLETED"),
            ("Partition", "gpu"),
        ]);
        let batch_line = accounting_line(&[
            ("JobID", "12345.batch"),
            ("State", "COMPLETED"),
            ("MaxRSS", "1000000K"),
        ]);
        let output = format!("{job_line}\n{batch_line}\n");

        let records = parse_accounting_output(output.as_bytes()).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["JobID"], "12345");
        assert_eq!(records[0]["Partition"], "gpu");
        assert_eq!(records[1]["JobID"], "12345.batch");
        assert_eq!(records[1]["MaxRSS"], "1000000K");
    }

    #[test]
    fn blank_lines_are_ignored() {
        let line = accounting_line(&[("JobID", "1"), ("State", "COMPLETED")]);
        let output = format!("\n{line}\n\n");
        let records = parse_accounting_output(output.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn empty_output_is_retryable() {
        assert!(accounting_output_is_empty(b""));
        assert!(accounting_output_is_empty(b"\n  \n"));
    }

    #[test]
    fn populated_output_is_not_retryable() {
        let line = accounting_line(&[("JobID", "1"), ("State", "COMPLETED")]);
        assert!(!accounting_output_is_empty(format!("{line}\n").as_bytes()));
    }
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p wdl-engine --lib backend::slurm_apptainer::tests`
Expected: 4 tests PASS (`parses_accounting_output_into_one_record_per_line`, `blank_lines_are_ignored`, `empty_output_is_retryable`, `populated_output_is_not_retryable`).

- [ ] **Step 7: Run fmt, and check (not full clippy) for compilation**

Run: `cargo --locked fmt -- --check && cargo check -p wdl-engine --tests`
Expected: no diffs, no errors.

Note: `cargo clippy --deny warnings` is expected to FAIL at this point with
8 `dead_code` errors — every item this task adds (`ACCOUNTING_FILE_NAME`,
`ACCOUNTING_FIELDS`, the three retry constants, `read_job_accounting`,
`parse_accounting_output`, `accounting_output_is_empty`) has no caller
outside the new tests until Task 3 wires `Monitor::read_job_accounting`
and `parse_accounting_output` into `execute()`. This is expected and
temporary — do not add `#[allow(dead_code)]`. The full clippy gate is
Task 3's Step 5, once the caller exists.

- [ ] **Step 8: Commit**

```bash
git add crates/wdl-engine/src/backend/slurm_apptainer.rs
git commit -m "feat(slurm): add sacct accounting fetch and parsing"
```

---

### Task 3: Wire the Slurm accounting dump into `execute()`

**Files:**
- Modify: `crates/wdl-engine/src/backend/slurm_apptainer.rs:1085-1111` (inside `execute()`)

**Interfaces:**
- Consumes: `Monitor::read_job_accounting` and `parse_accounting_output` (Task 2), `SlurmApptainerBackendConfig.job_accounting` (Task 1).
- Produces: `Monitor::write_job_accounting(job_id: u64, attempt_dir: &Path)` (async, returns `()`), used only within this file.

- [ ] **Step 1: Add `Monitor::write_job_accounting`**

In `crates/wdl-engine/src/backend/slurm_apptainer.rs`, find the end of `Monitor::read_job_accounting` you added in Task 2 (the closing of the outer function, right before `read_jobs`):

```rust
        Retry::spawn_notify(
            ExponentialBackoff::from_millis(ACCOUNTING_RETRY_INITIAL_DELAY_MS)
                .max_delay_millis(ACCOUNTING_RETRY_MAX_DELAY_MS)
                .take(ACCOUNTING_RETRY_ATTEMPTS),
            || try_read(job_id),
            |e: &anyhow::Error, _| {
                warn!(e = %e, "retrying `sacct` accounting query for Slurm job `{job_id}`");
            },
        )
        .await
    }

    /// Reads the current jobs using `sacct`.
```

Insert a new method between them:

```rust
        Retry::spawn_notify(
            ExponentialBackoff::from_millis(ACCOUNTING_RETRY_INITIAL_DELAY_MS)
                .max_delay_millis(ACCOUNTING_RETRY_MAX_DELAY_MS)
                .take(ACCOUNTING_RETRY_ATTEMPTS),
            || try_read(job_id),
            |e: &anyhow::Error, _| {
                warn!(e = %e, "retrying `sacct` accounting query for Slurm job `{job_id}`");
            },
        )
        .await
    }

    /// Best-effort: gathers final accounting information for a terminated job
    /// and writes it to [`ACCOUNTING_FILE_NAME`] in the task's attempt
    /// directory.
    ///
    /// Failures are logged and otherwise ignored; this must never affect the
    /// task's own result.
    async fn write_job_accounting(job_id: u64, attempt_dir: &Path) {
        let output = match Self::read_job_accounting(job_id).await {
            Ok(output) => output,
            Err(e) => {
                warn!("failed to gather Slurm accounting information for job `{job_id}`: {e:#}");
                return;
            }
        };

        let records = match parse_accounting_output(&output) {
            Ok(records) => records,
            Err(e) => {
                warn!("failed to parse Slurm accounting information for job `{job_id}`: {e:#}");
                return;
            }
        };

        let contents = match serde_json::to_vec_pretty(&records) {
            Ok(contents) => contents,
            Err(e) => {
                warn!(
                    "failed to serialize Slurm accounting information for job `{job_id}`: {e:#}"
                );
                return;
            }
        };

        let path = attempt_dir.join(ACCOUNTING_FILE_NAME);
        if let Err(e) = fs::write(&path, contents).await {
            warn!(
                path = %path.display(),
                "failed to write Slurm accounting information for job `{job_id}`: {e:#}"
            );
        }
    }

    /// Reads the current jobs using `sacct`.
```

- [ ] **Step 2: Call it from `execute()`, gated by config**

Find:

```rust
                result = job.completed => match result.context("failed to wait for task to complete")? {
                    Ok(exit_code) => {
                        let exit_status = exit_code.into_exit_status();
```

Replace with:

```rust
                result = job.completed => {
                    if backend_config.job_accounting.unwrap_or(true) {
                        Monitor::write_job_accounting(job_id, request.attempt_dir).await;
                    }

                    match result.context("failed to wait for task to complete")? {
                    Ok(exit_code) => {
                        let exit_status = exit_code.into_exit_status();
```

Then find the end of that same `match` (it currently closes the `select!` arm directly):

```rust
                        return Err(e);
                    }
                }
            };
```

Replace with (adding the extra closing brace for the new `{ ... }` block wrapping the `match`):

```rust
                        return Err(e);
                    }
                }
                }
            };
```

- [ ] **Step 3: Run `cargo fmt` to fix indentation**

Run: `cargo --locked fmt -p wdl-engine`

This is expected to reformat the block from Steps 1-2 into consistent indentation — inspect the diff afterward and confirm the braces still match Rust's structure (one `match` nested inside one `if`-guarded block, itself the body of the `result = job.completed =>` arm).

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p wdl-engine`
Expected: no errors. (`job_id` and `request` are already in scope at this point in `execute()` — `job_id` from `let job_id = job.id;` earlier, `request` is the function parameter.)

- [ ] **Step 5: Run clippy**

Run: `cargo --locked clippy -p wdl-engine --tests --all-features -- --deny warnings`
Expected: no warnings.

- [ ] **Step 6: Manual verification (no live Slurm cluster in CI)**

This module has no CLI-mocking test harness (module doc: "currently tested by hand"). If you have access to a Slurm cluster with the `slurm_apptainer` backend configured (see `test-configs/slurm-apptainer-engine.toml`), run a task through `sprocket run` and confirm `sacct.json` appears in the task's attempt directory (`<run_dir>/.../attempts/<n>/sacct.json`) with a non-empty JSON array. If no cluster is available, skip this step and note it in the PR description.

- [ ] **Step 7: Commit**

```bash
git add crates/wdl-engine/src/backend/slurm_apptainer.rs
git commit -m "feat(slurm): write sacct accounting dump on job completion"
```

---

### Task 4: LSF accounting fetch/parse (pure functions + tests)

**Files:**
- Modify: `crates/wdl-engine/src/backend/lsf_apptainer.rs`
- Test: same file, existing `#[cfg(test)] mod tests` block (`lsf_apptainer.rs:1041-1052`)

**Interfaces:**
- Consumes: nothing new from other tasks.
- Produces: `Monitor::read_job_accounting(job_id: u64) -> Result<Vec<serde_json::Value>>` (async), `parse_accounting_output(output: &[u8]) -> Result<Vec<serde_json::Value>>`, `ACCOUNTING_FIELDS: &str`, `ACCOUNTING_FILE_NAME: &str`. Task 5 calls `Monitor::read_job_accounting`.

- [ ] **Step 1: Add the new imports**

In `crates/wdl-engine/src/backend/lsf_apptainer.rs`, find:

```rust
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
```

Replace with:

```rust
use tokio::time::MissedTickBehavior;
use tokio_retry2::Retry;
use tokio_retry2::RetryError;
use tokio_retry2::strategy::ExponentialBackoff;
use tokio_util::sync::CancellationToken;
```

- [ ] **Step 2: Add the new constants**

Find the LSF equivalent of the Slurm monitor-interval/concurrency constants (search for `MONITOR_TAG_LENGTH` or the constant block near the top of the file — insert immediately after whichever default-concurrency constant already exists there, in the same style as Task 2 Step 2 for Slurm):

```rust
/// The name of the file where a job's final accounting information (from
/// `bjobs`) is written.
const ACCOUNTING_FILE_NAME: &str = "bjobs.json";

/// The fields requested from `bjobs` when gathering final accounting
/// information for a single terminated job.
///
/// This is a superset of the fields used for state-polling (`bjobs -json -o
/// "jobid stat exit_code max_mem avg_mem cpu_used ru_utime ru_stime"`,
/// `lsf_apptainer.rs:633`), since this query is only made once per job (on
/// termination) rather than on every monitor tick.
///
/// LSF has no reliably version-stable GPU/generic-resource equivalent to
/// Slurm's `TRES` fields, so none is requested here.
const ACCOUNTING_FIELDS: &str = "jobid stat exit_code max_mem avg_mem cpu_used ru_utime \
     ru_stime submit_time start_time finish_time exec_host queue job_name";

/// The initial delay, in milliseconds, before retrying a failed or
/// incomplete accounting query.
const ACCOUNTING_RETRY_INITIAL_DELAY_MS: u64 = 500;

/// The maximum delay, in milliseconds, between accounting query retries.
const ACCOUNTING_RETRY_MAX_DELAY_MS: u64 = 5_000;

/// The maximum number of attempts made to gather a job's accounting
/// information before giving up.
const ACCOUNTING_RETRY_ATTEMPTS: usize = 5;
```

- [ ] **Step 3: Add `parse_accounting_output`**

Find the end of `read_job_records` (the closing `}` of that function, still inside `impl Monitor`):

```rust
        Ok(serde_json::from_str::<Output>(
            str::from_utf8(&output.stdout).map_err(|_| anyhow!("`bjobs` output was not UTF-8"))?,
        )
        .context("failed to deserialize `bjobs` output")?
        .records)
    }
}
```

Replace with:

```rust
        Ok(serde_json::from_str::<Output>(
            str::from_utf8(&output.stdout).map_err(|_| anyhow!("`bjobs` output was not UTF-8"))?,
        )
        .context("failed to deserialize `bjobs` output")?
        .records)
    }
}

/// Deserializes the JSON output of a `bjobs -json` accounting query (using
/// [`ACCOUNTING_FIELDS`]) into the job's accounting records.
///
/// Kept separate from [`Monitor::read_job_records`] since that one is typed
/// to the polling-specific `JobRecord` shape, while this is used only for
/// the best-effort `bjobs.json` accounting dump and keeps each record as a
/// generic JSON value.
fn parse_accounting_output(output: &[u8]) -> Result<Vec<serde_json::Value>> {
    #[derive(Deserialize)]
    struct Output {
        /// The output records.
        #[serde(rename = "RECORDS")]
        records: Vec<serde_json::Value>,
    }

    Ok(serde_json::from_slice::<Output>(output)
        .context("failed to deserialize `bjobs` output")?
        .records)
}
```

- [ ] **Step 4: Add `Monitor::read_job_accounting`**

Find (still inside `impl Monitor`, directly before `read_job_records`):

```rust
    /// Reads the current job records using `bjobs`.
    async fn read_job_records(search_prefix: &str) -> Result<Vec<JobRecord>> {
```

Replace with:

```rust
    /// Reads final accounting information for a single terminated job using
    /// `bjobs`, retrying briefly since LSF's accounting data can lag behind
    /// job termination.
    async fn read_job_accounting(job_id: u64) -> Result<Vec<serde_json::Value>> {
        async fn try_read(
            job_id: u64,
        ) -> Result<Vec<serde_json::Value>, RetryError<anyhow::Error>> {
            let mut command = Command::new("bjobs");
            let command = command
                .arg("-a")
                .arg("-json")
                .arg("-o")
                .arg(ACCOUNTING_FIELDS)
                .arg(job_id.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            trace!(?command, "spawning `bjobs` to gather job accounting information");

            let child = command
                .spawn()
                .context("failed to spawn `bjobs` command")
                .map_err(RetryError::transient)?;

            let output = child
                .wait_with_output()
                .await
                .context("failed to wait for `bjobs` to exit")
                .map_err(RetryError::transient)?;
            if !output.status.success() {
                return Err(RetryError::transient(anyhow!(
                    "`bjobs` failed: {status}: {stderr}",
                    status = output.status,
                    stderr = str::from_utf8(&output.stderr)
                        .unwrap_or("<output not UTF-8>")
                        .trim()
                )));
            }

            let records =
                parse_accounting_output(&output.stdout).map_err(RetryError::transient)?;
            if records.is_empty() {
                return Err(RetryError::transient(anyhow!(
                    "`bjobs` returned no accounting records for LSF job `{job_id}`"
                )));
            }

            Ok(records)
        }

        Retry::spawn_notify(
            ExponentialBackoff::from_millis(ACCOUNTING_RETRY_INITIAL_DELAY_MS)
                .max_delay_millis(ACCOUNTING_RETRY_MAX_DELAY_MS)
                .take(ACCOUNTING_RETRY_ATTEMPTS),
            || try_read(job_id),
            |e: &anyhow::Error, _| {
                warn!(e = %e, "retrying `bjobs` accounting query for LSF job `{job_id}`");
            },
        )
        .await
    }

    /// Reads the current job records using `bjobs`.
    async fn read_job_records(search_prefix: &str) -> Result<Vec<JobRecord>> {
```

- [ ] **Step 5: Write the failing tests**

Find the existing test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_name_truncates() {
        let name = "é".repeat(LSF_JOB_NAME_MAX_LENGTH);
        assert_eq!(name.len(), 8188);
        let name = truncate_job_name(&name);
        assert!(name.len() < LSF_JOB_NAME_MAX_LENGTH);
    }
}
```

Replace with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_name_truncates() {
        let name = "é".repeat(LSF_JOB_NAME_MAX_LENGTH);
        assert_eq!(name.len(), 8188);
        let name = truncate_job_name(&name);
        assert!(name.len() < LSF_JOB_NAME_MAX_LENGTH);
    }

    #[test]
    fn parses_accounting_records() {
        let output = br#"{"RECORDS":[{"JOBID":"12345","STAT":"DONE","MAX_MEM":"512M"}]}"#;
        let records = parse_accounting_output(output).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["JOBID"], "12345");
        assert_eq!(records[0]["MAX_MEM"], "512M");
    }

    #[test]
    fn empty_records_array_parses_to_empty_vec() {
        let output = br#"{"RECORDS":[]}"#;
        let records = parse_accounting_output(output).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_accounting_output(b"not json").is_err());
    }
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p wdl-engine --lib backend::lsf_apptainer::tests`
Expected: 4 tests PASS (`job_name_truncates`, `parses_accounting_records`, `empty_records_array_parses_to_empty_vec`, `invalid_json_is_an_error`).

- [ ] **Step 7: Run fmt, and check (not full clippy) for compilation**

Run: `cargo --locked fmt -- --check && cargo check -p wdl-engine --tests`
Expected: no diffs, no errors.

Note: `cargo clippy --deny warnings` is expected to FAIL at this point with
dead_code errors on the items this task adds (`ACCOUNTING_FILE_NAME`,
`ACCOUNTING_FIELDS`, the three retry constants, `read_job_accounting`) —
they have no caller outside the new tests until Task 5 wires
`Monitor::read_job_accounting` into `execute()`. This is expected and
temporary — do not add `#[allow(dead_code)]`. The full clippy gate is
Task 5's Step 6, once the caller exists.

- [ ] **Step 8: Commit**

```bash
git add crates/wdl-engine/src/backend/lsf_apptainer.rs
git commit -m "feat(lsf): add bjobs accounting fetch and parsing"
```

---

### Task 5: Wire the LSF accounting dump into `execute()`, changelog, final verification

**Files:**
- Modify: `crates/wdl-engine/src/backend/lsf_apptainer.rs:979-1015` (inside `execute()`)
- Modify: `crates/wdl-engine/CHANGELOG.md`

**Interfaces:**
- Consumes: `Monitor::read_job_accounting` (Task 4), `LsfApptainerBackendConfig.job_accounting` (Task 1).
- Produces: `Monitor::write_job_accounting(job_id: u64, attempt_dir: &Path)` (async, returns `()`), used only within this file.

- [ ] **Step 1: Add `Monitor::write_job_accounting`**

In `crates/wdl-engine/src/backend/lsf_apptainer.rs`, find the end of `Monitor::read_job_accounting` (added in Task 4), right before `read_job_records`:

```rust
        Retry::spawn_notify(
            ExponentialBackoff::from_millis(ACCOUNTING_RETRY_INITIAL_DELAY_MS)
                .max_delay_millis(ACCOUNTING_RETRY_MAX_DELAY_MS)
                .take(ACCOUNTING_RETRY_ATTEMPTS),
            || try_read(job_id),
            |e: &anyhow::Error, _| {
                warn!(e = %e, "retrying `bjobs` accounting query for LSF job `{job_id}`");
            },
        )
        .await
    }

    /// Reads the current job records using `bjobs`.
```

Insert a new method between them:

```rust
        Retry::spawn_notify(
            ExponentialBackoff::from_millis(ACCOUNTING_RETRY_INITIAL_DELAY_MS)
                .max_delay_millis(ACCOUNTING_RETRY_MAX_DELAY_MS)
                .take(ACCOUNTING_RETRY_ATTEMPTS),
            || try_read(job_id),
            |e: &anyhow::Error, _| {
                warn!(e = %e, "retrying `bjobs` accounting query for LSF job `{job_id}`");
            },
        )
        .await
    }

    /// Best-effort: gathers final accounting information for a terminated job
    /// and writes it to [`ACCOUNTING_FILE_NAME`] in the task's attempt
    /// directory.
    ///
    /// Failures are logged and otherwise ignored; this must never affect the
    /// task's own result.
    async fn write_job_accounting(job_id: u64, attempt_dir: &Path) {
        let records = match Self::read_job_accounting(job_id).await {
            Ok(records) => records,
            Err(e) => {
                warn!("failed to gather LSF accounting information for job `{job_id}`: {e:#}");
                return;
            }
        };

        let contents = match serde_json::to_vec_pretty(&records) {
            Ok(contents) => contents,
            Err(e) => {
                warn!("failed to serialize LSF accounting information for job `{job_id}`: {e:#}");
                return;
            }
        };

        let path = attempt_dir.join(ACCOUNTING_FILE_NAME);
        if let Err(e) = fs::write(&path, contents).await {
            warn!(
                path = %path.display(),
                "failed to write LSF accounting information for job `{job_id}`: {e:#}"
            );
        }
    }

    /// Reads the current job records using `bjobs`.
```

- [ ] **Step 2: Call it from `execute()`, gated by config**

Find:

```rust
                result = job.completed => match result.context("failed to wait for task to complete")? {
                    Ok(exit_code) => {
                        // See WEXITSTATUS from wait(2) to explain the shift and masking here
```

Replace with:

```rust
                result = job.completed => {
                    if backend_config.job_accounting.unwrap_or(true) {
                        Monitor::write_job_accounting(job_id, request.attempt_dir).await;
                    }

                    match result.context("failed to wait for task to complete")? {
                    Ok(exit_code) => {
                        // See WEXITSTATUS from wait(2) to explain the shift and masking here
```

Then find the end of that same `match`:

```rust
                        return Err(e);
                    }
                }
            };
```

Replace with:

```rust
                        return Err(e);
                    }
                }
                }
            };
```

- [ ] **Step 3: Run `cargo fmt` to fix indentation**

Run: `cargo --locked fmt -p wdl-engine`

Inspect the diff afterward, same as Task 3 Step 3.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p wdl-engine`
Expected: no errors.

- [ ] **Step 5: Add the changelog entry**

In `crates/wdl-engine/CHANGELOG.md`, find:

```markdown
## Unreleased

## 0.17.0 - 2026-07-15
```

Replace with:

```markdown
## Unreleased

#### Added

* Added a `job_accounting` configuration option (defaults to `true`) to the Slurm and LSF Apptainer backends: once a task's job reaches a terminal state, the backend gathers accounting information via `sacct`/`bjobs` and writes it to `sacct.json`/`bjobs.json` in the task's attempt directory. This is best-effort and never affects the task's own result.

## 0.17.0 - 2026-07-15
```

- [ ] **Step 6: Run the full test suite and lints**

Run:

```bash
cargo test -p wdl-engine --lib backend::
cargo test -p sprocket --lib config::test
cargo --locked fmt -- --check
cargo --locked clippy --workspace --tests --all-features -- --deny warnings
```

Expected: all PASS, no fmt diff, no clippy warnings.

- [ ] **Step 7: Manual verification (no live LSF cluster in CI)**

Same caveat as Task 3 Step 6 — if you have access to an LSF cluster with the `lsf_apptainer` backend configured, run a task and confirm `bjobs.json` appears in the attempt directory with a non-empty JSON array. Otherwise skip and note it in the PR description.

- [ ] **Step 8: Commit**

```bash
git add crates/wdl-engine/src/backend/lsf_apptainer.rs crates/wdl-engine/CHANGELOG.md
git commit -m "feat(lsf): write bjobs accounting dump on job completion"
```
