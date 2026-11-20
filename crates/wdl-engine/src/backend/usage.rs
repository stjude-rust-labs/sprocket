//! Backend-owned container accounting and native-usage selection.

use std::future::Future;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use crankshaft::events::Event;
use crankshaft::events::TaskId;
use crankshaft::events::TaskResourceUsage;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::TryRecvError;
use tracing::warn;

/// POSIX wrapper shell; the configured task shell starts only for the command.
pub(super) const WRAPPER_SHELL: &str = "/bin/sh";

/// Runs the original command in its own shell so its traps, `exec`, and `$$`
/// retain their meaning. The recording exists even when counters are absent.
const EPILOGUE: &str = r#"if ! ( : > "$3" ) 2>/dev/null; then
    exec "$1" "$2"
fi
"$1" "$2"
sprocket_status=$?
{
    if [ -r /sys/fs/cgroup/cgroup.controllers ] &&
       [ -r /sys/fs/cgroup/cpu.stat ] &&
       [ -r /sys/fs/cgroup/memory.peak ]; then
        IFS= read -r sprocket_group < /proc/self/cgroup || sprocket_group=
        if [ "$sprocket_group" = "0::/" ]; then
            {
                while read -r key value rest; do
                    case "$key" in
                        usage_usec|user_usec|system_usec) printf '%s %s\n' "$key" "$value";;
                    esac
                done < /sys/fs/cgroup/cpu.stat
                IFS= read -r peak < /sys/fs/cgroup/memory.peak &&
                    printf 'memory_peak_bytes %s\n' "$peak"
            } > "$3"
        fi
    fi
} 2>/dev/null
exit "$sprocket_status"
"#;

/// Arguments for the wrapper shell, without interpolating paths or commands.
pub(super) fn command_args(shell: &str, command: &str, recording: &str) -> Vec<String> {
    ["-c", EPILOGUE, "sprocket-usage", shell, command, recording]
        .map(str::to_string)
        .into()
}

/// Parses a complete cgroup-v2 recording; an empty file denotes unavailable
/// counters, not a zero-valued measurement.
fn parse_recording(contents: &str) -> Result<TaskResourceUsage> {
    if contents.trim().is_empty() {
        return Ok(TaskResourceUsage::default());
    }
    let mut total = None;
    let mut user = None;
    let mut system = None;
    let mut memory = None;
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_whitespace();
        let key = fields.next().context("missing cgroup counter name")?;
        let value = fields
            .next()
            .context("missing cgroup counter value")?
            .parse::<u64>()
            .with_context(|| format!("invalid cgroup counter `{key}`"))?;
        ensure!(fields.next().is_none(), "invalid cgroup counter `{key}`");
        let target = match key {
            "usage_usec" => &mut total,
            "user_usec" => &mut user,
            "system_usec" => &mut system,
            "memory_peak_bytes" => &mut memory,
            _ => bail!("unknown cgroup counter `{key}`"),
        };
        ensure!(
            target.replace(value).is_none(),
            "duplicate cgroup counter `{key}`"
        );
    }
    let mut usage = TaskResourceUsage::default();
    usage.cpu_time_ms = Some(total.context("missing total CPU counter")? / 1000);
    usage.user_cpu_time_ms = Some(user.context("missing user CPU counter")? / 1000);
    usage.system_cpu_time_ms = Some(system.context("missing system CPU counter")? / 1000);
    usage.max_memory = Some(memory.context("missing memory peak counter")?);
    Ok(usage)
}

/// Observes one invocation on the original shared event stream.
pub(super) struct UsageEvents {
    /// Original event stream for consumers.
    upstream: Option<broadcast::Sender<Event>>,
    /// Original receiver polled alongside the backend future.
    receiver: Option<broadcast::Receiver<Event>>,
    /// Unique requested execution name identifying this invocation.
    name: String,
    /// Task identity reported by this invocation.
    id: Option<TaskId>,
    /// Whether this invocation reported nonempty native usage.
    native: bool,
    /// Whether observation is complete and unambiguous.
    reliable: bool,
}

impl UsageEvents {
    /// Creates an observer only when events have a consumer.
    pub fn new(upstream: Option<&broadcast::Sender<Event>>, name: &str) -> Self {
        Self {
            upstream: upstream.cloned(),
            receiver: upstream.map(broadcast::Sender::subscribe),
            name: name.to_string(),
            id: None,
            native: false,
            reliable: true,
        }
    }

    /// Requires a known task, complete observation, and no native usage.
    pub fn needs_fallback(&self) -> bool {
        self.reliable && !self.native && self.id.is_some()
    }

    /// Records only this invocation's identity and source-selection state.
    fn observe(&mut self, event: Event) {
        match &event {
            Event::TaskCreated { id, name, .. } if *name == self.name => {
                if self.id.is_some_and(|prior| prior != *id) {
                    self.reliable = false;
                    warn!("multiple task IDs observed for one execution; omitting fallback usage");
                }
                self.id = Some(*id);
            }
            Event::TaskResourceUsage { id, usage } if Some(*id) == self.id => {
                self.native |= !usage.is_empty();
            }
            _ => {}
        }
    }

    /// Disables fallback when native usage may have been lost.
    fn lost(&mut self, count: u64) {
        self.reliable = false;
        warn!(
            count,
            "backend event observer lagged; omitting fallback resource usage"
        );
    }

    /// Observes through completion, including events queued at return.
    pub async fn run<F: Future>(&mut self, future: F) -> F::Output {
        let Some(mut receiver) = self.receiver.take() else {
            return future.await;
        };
        tokio::pin!(future);
        let result = loop {
            tokio::select! {
                event = receiver.recv() => match event {
                    Ok(event) => self.observe(event),
                    Err(broadcast::error::RecvError::Lagged(count)) => self.lost(count),
                    Err(broadcast::error::RecvError::Closed) => break future.await,
                },
                result = &mut future => break result,
            }
        };
        loop {
            match receiver.try_recv() {
                Ok(event) => self.observe(event),
                Err(TryRecvError::Lagged(count)) => self.lost(count),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
        result
    }

    /// Publishes valid, available fallback usage only when native usage is
    /// absent.
    pub fn publish(&self, contents: &str) {
        if !self.needs_fallback() {
            return;
        }
        match parse_recording(contents) {
            Ok(usage) if !usage.is_empty() => {
                if let (Some(sender), Some(id)) = (&self.upstream, self.id) {
                    let _ = sender.send(Event::TaskResourceUsage { id, usage });
                }
            }
            Ok(_) => warn!("task-scoped cgroup-v2 counters are unavailable"),
            Err(error) => warn!("invalid container resource usage recording: {error:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORDING: &str =
        "usage_usec 5200349\nuser_usec 4100000\nsystem_usec 1100349\nmemory_peak_bytes 67108864\n";

    #[test]
    fn cgroup_recording_normalizes_units_and_rejects_incomplete_data() {
        let usage = parse_recording(RECORDING).unwrap();
        assert_eq!(usage.cpu_time_ms, Some(5200));
        assert_eq!(usage.user_cpu_time_ms, Some(4100));
        assert_eq!(usage.system_cpu_time_ms, Some(1100));
        assert_eq!(usage.max_memory, Some(67108864));
        assert!(usage.avg_memory.is_none());
        assert!(parse_recording("").unwrap().is_empty());
        for contents in [
            "usage_usec -1\n",
            "usage_usec 18446744073709551616\n",
            "usage_usec 1000\nmemory_peak_bytes 40\n",
        ] {
            assert!(parse_recording(contents).is_err(), "{contents}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn wrapper_does_not_depend_on_task_path() {
        let root = tempfile::tempdir().unwrap();
        let command = root.path().join("command");
        std::fs::write(&command, "printf hello; exit 7").unwrap();
        let recording = root.path().join("recording");
        let output = std::process::Command::new(WRAPPER_SHELL)
            .args(command_args(
                "/bin/sh",
                command.to_str().unwrap(),
                recording.to_str().unwrap(),
            ))
            .env("PATH", root.path())
            .output();
        assert!(output.is_ok(), "wrapper could not start: {output:?}");
        let output = output.unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"hello");
        assert!(output.stderr.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn unavailable_recording_does_not_prevent_command_execution() {
        use std::fs;
        use std::process::Command;

        let root = tempfile::tempdir().unwrap();
        let command = root.path().join("command");
        fs::write(&command, "printf hello; exit 7").unwrap();
        let recording = root.path().join("missing/recording");
        let output = Command::new("sh")
            .args(command_args(
                "sh",
                command.to_str().unwrap(),
                recording.to_str().unwrap(),
            ))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"hello");
        assert!(output.stderr.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn epilogue_preserves_commands_and_omits_unsupported_cgroups() {
        use std::fs;
        use std::process::Command;

        let root = tempfile::tempdir().unwrap();
        let cgroup = root.path().join("cgroup");
        fs::create_dir(&cgroup).unwrap();
        let membership = root.path().join("membership");
        let command = root.path().join("command 'quoted'");
        let recording = root.path().join("usage 'quoted'");
        for (payload, code, stderr) in [
            (
                "trap 'printf trapped >&2' EXIT; printf hello; exit 7",
                7,
                "trapped",
            ),
            ("printf hello; exec sh -c 'exit 9'", 9, ""),
        ] {
            for supported in [true, false] {
                fs::write(&command, payload).unwrap();
                fs::write(
                    &membership,
                    if supported {
                        "0::/\n"
                    } else {
                        "0::/host/task\n"
                    },
                )
                .unwrap();
                fs::write(cgroup.join("cgroup.controllers"), "cpu memory\n").unwrap();
                fs::write(
                    cgroup.join("cpu.stat"),
                    "usage_usec 5200349\nuser_usec 4100000\nsystem_usec 1100349\n",
                )
                .unwrap();
                fs::write(cgroup.join("memory.peak"), "67108864\n").unwrap();
                let mut args =
                    command_args("sh", command.to_str().unwrap(), recording.to_str().unwrap());
                args[1] = args[1]
                    .replace("/sys/fs/cgroup", cgroup.to_str().unwrap())
                    .replace("/proc/self/cgroup", membership.to_str().unwrap());
                let output = Command::new("sh").args(args).output().unwrap();
                assert_eq!(output.status.code(), Some(code));
                assert_eq!(output.stdout, b"hello");
                assert_eq!(output.stderr, stderr.as_bytes());
                let contents = fs::read_to_string(&recording).unwrap();
                if supported {
                    assert_eq!(parse_recording(&contents).unwrap().cpu_time_ms, Some(5200));
                } else {
                    assert!(contents.is_empty());
                }
            }
        }
        fs::write(&membership, "0::/\n").unwrap();
        fs::remove_file(cgroup.join("memory.peak")).unwrap();
        let mut args = command_args("sh", command.to_str().unwrap(), recording.to_str().unwrap());
        args[1] = args[1]
            .replace("/sys/fs/cgroup", cgroup.to_str().unwrap())
            .replace("/proc/self/cgroup", membership.to_str().unwrap());
        let output = Command::new("sh").args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(9));
        assert!(fs::read_to_string(&recording).unwrap().is_empty());
    }

    #[tokio::test]
    async fn original_channel_observes_only_requested_execution() {
        use tokio_util::sync::CancellationToken;

        let (events, mut receiver) = broadcast::channel(16);
        let mut observed = UsageEvents::new(Some(&events), "task");
        observed
            .run(async {
                for (id, name) in [(8, "other"), (7, "task"), (9, "concurrent")] {
                    events
                        .send(Event::TaskCreated {
                            id,
                            name: name.into(),
                            tes_id: None,
                            token: CancellationToken::new(),
                        })
                        .unwrap();
                }
                let mut usage = TaskResourceUsage::default();
                usage.cpu_time_ms = Some(0);
                events
                    .send(Event::TaskResourceUsage { id: 8, usage })
                    .unwrap();
            })
            .await;
        observed.publish(RECORDING);
        let mut measurements = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            if let Event::TaskResourceUsage { id, .. } = event {
                measurements.push(id);
            }
        }
        assert_eq!(measurements, vec![8, 7]);
    }

    #[tokio::test]
    async fn missing_identity_and_empty_recording_never_publish_usage() {
        use tokio_util::sync::CancellationToken;

        for requested_created in [false, true] {
            let (events, mut receiver) = broadcast::channel(16);
            let mut observed = UsageEvents::new(Some(&events), "task");
            observed
                .run(async {
                    events
                        .send(Event::TaskCreated {
                            id: 7,
                            name: if requested_created { "task" } else { "other" }.into(),
                            tes_id: None,
                            token: CancellationToken::new(),
                        })
                        .unwrap();
                })
                .await;
            observed.publish(if requested_created { "" } else { RECORDING });
            while let Ok(event) = receiver.try_recv() {
                assert!(!matches!(event, Event::TaskResourceUsage { .. }));
            }
        }
    }

    #[tokio::test]
    async fn queued_native_events_win_over_container_fallback() {
        use crankshaft::events::Event;
        use crankshaft::events::TaskResourceUsage;
        use tokio::sync::broadcast;
        use tokio_util::sync::CancellationToken;

        for native in [false, true] {
            let (events, mut receiver) = broadcast::channel(16);
            let mut observed = UsageEvents::new(Some(&events), "task");
            let sender = events.clone();
            let result = observed
                .run(async {
                    sender
                        .send(Event::TaskCreated {
                            id: 7,
                            name: "task".into(),
                            tes_id: None,
                            token: CancellationToken::new(),
                        })
                        .unwrap();
                    let mut usage = TaskResourceUsage::default();
                    if native {
                        usage.cpu_time_ms = Some(0);
                    }
                    sender
                        .send(Event::TaskResourceUsage { id: 7, usage })
                        .unwrap();
                    sender.send(Event::TaskCanceled { id: 7 }).unwrap();
                    9
                })
                .await;
            assert_eq!(result, 9);
            observed.publish(RECORDING);
            let mut measurements = Vec::new();
            let mut canceled = false;
            while let Ok(event) = receiver.try_recv() {
                match event {
                    Event::TaskResourceUsage { id, usage } if !usage.is_empty() => {
                        assert_eq!(id, 7);
                        measurements.push(usage.cpu_time_ms.unwrap());
                    }
                    Event::TaskCanceled { .. } => canceled = true,
                    _ => {}
                }
            }
            assert!(
                canceled,
                "lifecycle events must remain on the original stream"
            );
            assert_eq!(measurements, if native { vec![0] } else { vec![5200] });
        }
    }

    #[tokio::test]
    async fn lag_suppresses_container_usage_even_when_identity_is_recovered() {
        use crankshaft::events::Event;
        use tokio::sync::broadcast;
        use tokio::sync::broadcast::error::TryRecvError;
        use tokio_util::sync::CancellationToken;

        let (events, mut receiver) = broadcast::channel(16);
        let mut observed = UsageEvents::new(Some(&events), "task");
        let sender = events.clone();
        observed
            .run(async {
                for _ in 0..10000 {
                    sender.send(Event::TaskStarted { id: 7 }).unwrap();
                }
                sender
                    .send(Event::TaskCreated {
                        id: 7,
                        name: "task".into(),
                        tes_id: None,
                        token: CancellationToken::new(),
                    })
                    .unwrap();
            })
            .await;
        observed.publish(RECORDING);
        let mut published = 0;
        loop {
            match receiver.try_recv() {
                Ok(Event::TaskResourceUsage { id, .. }) => {
                    assert_eq!(id, 7);
                    published += 1;
                }
                Ok(_) | Err(TryRecvError::Lagged(_)) => {}
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
        assert_eq!(published, 0);
    }
}
