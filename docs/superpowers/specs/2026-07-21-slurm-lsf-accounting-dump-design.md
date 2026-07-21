# Slurm/LSF job accounting dump

## Context

The `slurm_apptainer` backend already shells out to `sacct` to poll job state
(`crates/wdl-engine/src/backend/slurm_apptainer.rs:733-765`), and the `sacct`
output is already parsed into a `JobRecord` that includes CPU-time and
virtual-memory fields (`slurm_apptainer.rs:262-283`). Once a job reaches a
terminal state, those fields are used for exactly one `debug!` log line
(`slurm_apptainer.rs:463-474`) and then discarded. The `lsf_apptainer` backend
has the identical shape: `bjobs -json` polling
(`crates/wdl-engine/src/backend/lsf_apptainer.rs:616-660`) parses memory/CPU
fields into a `JobRecord` (`lsf_apptainer.rs:163-190`) that are likewise only
logged (`lsf_apptainer.rs:317-328`) and dropped.

We want to stop throwing this away. After a Slurm or LSF job finishes
(successfully or not), gather as much accounting information as `sacct`/
`bjobs` will give us and make it available for later inspection, without
requiring the sprocket binary's SQLite layer or any new dependency.

## Why a file, not the database

`wdl-engine` (where these backends live) is storage-agnostic by design — it
has no dependency on the `sprocket` binary's SQLite `tasks` table
(`migrations/20251108201315_initial.up.sql:116-139`) or its `Database` trait
(`src/system/v1/db.rs`). The only channel backends have to the outside world
is `crankshaft`'s `Event` enum, which is a fixed vocabulary from an external
crates.io dependency (`crankshaft = "0.10.0"` in `Cargo.toml:49`, not a
workspace member) — `TaskCompleted`/`TaskFailed` have no slot for arbitrary
metadata, and extending them would mean forking an upstream crate.

Both backends already write several per-attempt files directly to
`request.attempt_dir` (`command`, `stdout`, `stderr`, `apptainer_command`,
`slurm.stdout`/`slurm.stderr` — see `crates/wdl-engine/src/backend.rs:227-253`
and `slurm_apptainer.rs:1013-1021`). Writing one more file there follows the
existing pattern exactly, needs no schema migration, and works identically
whether or not the sprocket binary's DB is even in play.

## Configuration

Each backend's config struct gets a new `job_accounting: bool` field,
opt-out, defaulting to `true`, following the existing `cleanup` field on
`DockerBackendConfig` (`config.rs:1516-1529`) as the pattern: a
`default_*_job_accounting() -> bool { true }` function referenced via
`#[toml(default = ...)]` / `#[schemars(default = "...")]`.

```toml
[backend.slurm_apptainer]
job_accounting = false  # opt out; defaults to true

[backend.lsf_apptainer]
job_accounting = false  # opt out; defaults to true
```

Added to `SlurmApptainerBackendConfig` (`config.rs:2545`) and
`LsfApptainerBackendConfig` (`config.rs:2317`). When `false`, the backend
skips the extra `sacct`/`bjobs` call and the JSON file entirely — no
behavior change beyond that from today.

## Trigger point

Both backends have the same `execute()` shape: after submitting the job, they
`tokio::select!` on cancellation vs. the job's completion oneshot
(`slurm_apptainer.rs:1070-1111`, analogous LSF code). The accounting dump is
gathered **only** on the completion-oneshot arm (`Ok(exit_code)` or
`Err(e)`) — i.e., only when the job reached a terminal state on its own
(completed, failed, timed out, OOM-killed, etc.). It is **not** gathered on
the cancellation branches (`task_token.cancelled()` / `token.cancelled()`,
which call `scancel`/`bkill` and return early) — a user-initiated cancellation
is out of scope for "the job finished."

The dump is best-effort: if the extra `sacct`/`bjobs` call fails (binary
missing, job already purged from the accounting DB, non-UTF8 output, etc.),
log a `warn!` and skip writing the file. It must never change the task's own
exit code or turn a successful/failed task result into a different outcome.

## Slurm: fields and format

A dedicated call, separate from the existing multi-job polling call (which
stays as-is, since it's tuned for polling many jobs cheaply in one shot):

```
sacct -P -n --format=<fields> -j <job_id>
```

Unlike the polling call, this one does **not** filter out job-step lines
(the existing polling code skips any job id containing `.`,
`slurm_apptainer.rs:413`). Slurm frequently only reports memory/IO stats on
the `.batch`/`.extern` step lines rather than the parent job line, so all
lines returned for the job id are kept.

Field list:

```
JobID,JobName,Partition,State,ExitCode,NodeList,Submit,Start,End,Elapsed,
AllocCPUS,ReqMem,ReqTRES,AllocTRES,MaxRSS,MaxVMSize,AveRSS,AveVMSize,
TotalCPU,UserCPU,SystemCPU,MaxDiskRead,MaxDiskWrite
```

(`ReqTRES`/`AllocTRES` capture GPU/generic-resource allocation.)

Each `sacct` line is parsed into a JSON object keyed by field name (values
kept as the raw strings `sacct` emits — no unit/duration parsing). The full
set of lines for the job is written as a JSON array to
`attempt_dir/sacct.json`.

## LSF: fields and format

The existing polling call already asks for JSON output
(`bjobs -json -o "..."`, `lsf_apptainer.rs:626-636`) and already deserializes
a `RECORDS` array. For the completion dump, widen the `-o` field list for a
single-job call and write the `RECORDS` array straight to
`attempt_dir/bjobs.json` — no custom parsing needed, since it's already JSON.

Field list:

```
jobid stat exit_code max_mem avg_mem cpu_used ru_utime ru_stime
submit_time start_time finish_time exec_host queue job_name
```

LSF has no reliably version-stable GPU/generic-resource equivalent to
Slurm's `TRES` fields, so none is included — this is a known gap, not a
silent mismatch with the Slurm side.

## Retrying on accounting lag

Slurm's `slurmdbd` (and LSF's accounting backend) can take a few seconds to
catch up after a job terminates, particularly for the `.batch` step's
memory/IO stats. Rather than write a bespoke retry loop, reuse
`tokio_retry2`, already a workspace dependency and already used for exactly
this shape of problem in `backend/apptainer.rs:327-345` (`Retry::spawn_notify`
with an `ExponentialBackoff` strategy, logging a `warn!` on each failed
attempt via the notify callback).

The accounting fetch is retried when:
- the `sacct`/`bjobs` invocation itself fails (spawn error, non-zero exit,
  non-UTF8 output), or
- the invocation succeeds but returns zero matching records for the job
  (the signal that the accounting DB hasn't caught up yet).

Backoff parameters are intentionally much shorter than the image-pull retry
(which budgets up to 60s for registry flakiness) since this is short-lived DB
propagation lag, not network flakiness: `ExponentialBackoff::from_millis(500)
.max_delay_millis(5_000).take(5)` — a handful of attempts capped at a few
seconds each, a few seconds of total worst-case added latency after job
completion.

## Error handling

- If `job_accounting` is `false`, skip the extra call and file write
  entirely — not treated as an error, just a no-op.
- If all retries are exhausted (command keeps failing, or the accounting DB
  never returns records within the retry budget): log a `warn!`, skip
  writing the file.
- Parse/deserialize failure on otherwise-successful output: same — `warn!`
  and skip.
- None of the above ever change the `TaskExecutionResult` or cause the task
  to be reported as failed.

## Testing

Neither backend has any live-cluster test infrastructure today (both module
docs say "tested by hand"; `lsf_apptainer.rs:1041-1052` has exactly one
plain `#[test]` unit test as the only existing precedent). Add unit tests
that feed canned `sacct`/`bjobs` output text through the new parsing
function and assert the resulting JSON shape, following that same pattern.
Also unit test the "should this be retried" predicate (empty output vs. a
populated record set) directly, since that's the one piece of new decision
logic; the retry/backoff mechanics themselves come from `tokio_retry2` and
don't need re-testing. No mocking framework, no integration harness.
