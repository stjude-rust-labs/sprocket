use super::*;

struct TestContext {
    source: String,
    dependencies: Vec<(String, String)>,
}

impl TestContext {
    fn new(source: String) -> Self {
        Self {
            source,
            dependencies: Vec::new(),
        }
    }

    fn with_dependency(mut self, filename: String, source: String) -> Self {
        self.dependencies.push((filename, source));
        self
    }

    async fn render(&self, depth: Option<usize>) -> String {
        const SOURCE_FILENAME: &str = "test.wdl";

        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join(SOURCE_FILENAME);

        for (filename, source) in [(SOURCE_FILENAME, &*self.source)].into_iter().chain(
            self.dependencies
                .iter()
                .map(|(filename, source)| (filename.as_str(), source.as_str())),
        ) {
            let path = directory.path().join(filename);
            std::fs::write(&path, source).unwrap();
        }

        let source: Source = source_path.to_string_lossy().parse().unwrap();
        let document = analyze_singular_source(
            &source,
            None,
            Default::default(),
            Default::default(),
            None,
            Default::default(),
            false,
        )
        .await
        .unwrap();

        let workflow = document
            .root()
            .ast()
            .into_v1()
            .unwrap()
            .workflows()
            .next()
            .unwrap();

        render_mermaid(&workflow, &document, depth)
    }
}

#[test]
fn escapes_mermaid_label_entities() {
    assert_eq!(
        escape("~{bam} & < > \" #quot;"),
        "~{bam} #amp; #lt; #gt; #quot; #35;quot;"
    );
    assert_eq!(escape("a &&\n        b"), "a #amp;#amp; b");
}

#[tokio::test]
async fn expands_workflows_below_the_depth_cap() {
    let source = r#"version 1.0
import "child.wdl" as child

workflow root {
    scatter (item in [1]) {
        call child.child
    }
}
"#;

    let ctx = TestContext::new(source.into()).with_dependency(
        "child.wdl".into(),
        r#"version 1.0
task noop {
    command <<< echo noop >>>
}

workflow child {
    call noop
}
"#
        .into(),
    );

    let capped = ctx.render(Some(0)).await;

    assert!(capped.contains("n2[\"child<br/><i>child.child</i>\"]"));
    assert!(!capped.contains("subgraph n2[\"child (child.child)\"]"));
    assert!(
        ctx.render(Some(1))
            .await
            .contains("subgraph n2[\"child (child.child)\"]")
    );
}

#[tokio::test]
async fn renders_calls_from_their_dependencies() {
    let source = r#"
version 1.0

task produce {
    command <<<
        echo value
    >>>

    output {
        String value = stdout()
    }
}

task consume {
    input {
        String value
    }

    command <<<
        echo ~{value}
    >>>
}

workflow test {
    call produce
    call consume as left { input: value = produce.value }
    call consume as right { input: value = produce.value }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    assert!(diagram.contains("n0 --> n1"));
    assert!(diagram.contains("n0 --> n2"));
    assert!(!diagram.contains("n1 --> n2"));
}

#[tokio::test]
async fn preserves_both_paths_through_declaration_only_conditionals() {
    let source = r#"
version 1.0

task consume {
    input {
        String value
    }

    command <<< echo ~{value} >>>
}

workflow test {
    input {
        Boolean enabled
    }

    if (enabled) {
        String selected = "yes"
    }

    call consume { input: value = select_first([selected, "no"]) }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    assert!(diagram.contains("n0 -->|\"yes\"| n1"));
    assert!(diagram.contains("n0 -->|\"no\"| n1"));
}

#[tokio::test]
async fn preserves_fallthrough_from_multi_clause_conditionals() {
    let source = r#"
version 1.3

task consume {
    input {
        String value
    }

    command <<< echo ~{value} >>>
}

workflow test {
    input { String mode }
    if (mode == "a") {
        String selected = "a"
    } else if (mode == "b") {
        String selected = "b"
    }
    call consume { input: value = select_first([selected, "none"]) }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    assert!(diagram.contains("n0 -->|\"no\"| n1"));
}

#[tokio::test]
async fn quotes_edge_labels_containing_pipes() {
    let source = r#"
version 1.3

task noop {
    command <<< echo noop >>>
}

workflow test {
    input {
        Boolean a
        Boolean b
    }

    if (a || b) {
        call noop as first
    } else {
        call noop as second
    }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    assert!(diagram.contains("n0 -->|\"if a || b\"| n1"));
    assert!(diagram.contains("n0 -->|\"else\"| n2"));
}

#[tokio::test]
async fn depth_limited_calls_do_not_block_later_expansion() {
    let source = r#"version 1.0
import "child.wdl" as child
import "middle.wdl" as middle

workflow root {
    call middle.middle
    call child.child
}
"#;
    let middle = r#"version 1.0
import "child.wdl" as child

workflow middle {
    call child.child
}
"#;
    let child = r#"version 1.0

task noop {
    command <<< echo noop >>>
}

workflow child {
    call noop
}
"#;

    let ctx = TestContext::new(source.into())
        .with_dependency("middle.wdl".into(), middle.into())
        .with_dependency("child.wdl".into(), child.into());
    let diagram = ctx.render(Some(1)).await;

    // `middle` expands but its nested `child` call is depth-limited, so the
    // top-level `child` call must still expand.
    assert!(diagram.contains("subgraph n0[\"middle (middle.middle)\"]"));
    assert!(diagram.contains("n1[\"child<br/><i>child.child</i>\"]"));
    assert!(diagram.contains("subgraph n2[\"child (child.child)\"]"));
}

#[tokio::test]
async fn renders_deterministically() {
    let source = r#"
version 1.0

task produce {
    command <<< echo value >>>

    output {
        String value = stdout()
    }
}

task combine {
    input {
        String left
        String right
    }

    command <<< echo ~{left} ~{right} >>>
}

workflow test {
    call produce as left
    call produce as right
    call combine { input:
        left = left.value,
        right = right.value,
    }
}
"#;

    let ctx = TestContext::new(source.into());
    let expected = ctx.render(Some(0)).await;

    for _ in 0..16 {
        let ctx = TestContext::new(source.into());
        assert_eq!(ctx.render(Some(0)).await, expected);
    }
}

#[tokio::test]
async fn renders_multi_statement_conditionals_with_subgraph() {
    let source = r#"
version 1.0

task noop {
    command <<< echo noop >>>
}

workflow test {
    if (true) {
        call noop as first
        call noop as second
    } else {
        call noop as third
    }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    // The `if` branch has > 1 statement, so it creates a subgraph
    assert!(diagram.contains("subgraph n1[\"if true\"]"));
    assert!(diagram.contains("n0 -->|\"if true\"| n1"));
    // The else branch has 1 statement, so it connects directly from the
    // diamond.
    assert!(diagram.contains("n4[\"third<br/><i>noop</i>\"]"));
    assert!(diagram.contains("n0 -->|\"else\"| n4"));
}

#[tokio::test]
async fn renders_single_clause_conditional_with_subgraph() {
    let source = r#"
version 1.0

task noop {
    command <<< echo noop >>>
}

workflow test {
    if (true) {
        call noop as first
        call noop as second
    }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    // The yes branch has > 1 statement, so it creates a subgraph.
    assert!(diagram.contains("subgraph n1[\"then\"]"));
    assert!(diagram.contains("n0 -->|\"yes\"| n1"));
    assert!(diagram.contains("n2[\"first<br/><i>noop</i>\"]"));
    assert!(diagram.contains("n3[\"second<br/><i>noop</i>\"]"));
    assert!(!diagram.contains("n2 --> n3"));
}

#[tokio::test]
async fn renders_single_statement_conditionals_without_subgraph() {
    let source = r#"
version 1.0

task noop {
    command <<< echo noop >>>
}

workflow test {
    if (true) {
        call noop as only
    }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    // Single statement connects directly from diamond without a subgraph.
    assert!(diagram.contains("n0 -->|\"yes\"| n1"));
    assert!(diagram.contains("n1[\"only<br/><i>noop</i>\"]"));
    assert!(!diagram.contains("subgraph"));
}

#[tokio::test]
async fn renders_independent_calls_separately() {
    let source = r#"
version 1.0

task noop {
    command <<< echo noop >>>
}

workflow main {
    scatter (foo in []) {
        call noop as first
    }

    call noop as second
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    // Nothing should connect to `n3` (`second`)
    assert!(!diagram.contains("--> n3"));
}

#[tokio::test]
async fn renders_calls_dependent_on_scatter() {
    let source = r#"
version 1.0

task produce {
    command <<< echo 1 >>>

    output {
        Int x = 1
    }
}

task consume {
    input {
        Array[Int] xs
    }

    command <<< echo ~{sep=' ' xs} >>>
}

workflow main {
    scatter (foo in [1, 2]) {
        call produce as first
    }

    call consume as second { input: xs = first.x }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    // `second` consumes output from `first` gathered across the scatter,
    // so `first` connects directly to `second`.
    assert!(diagram.contains("n2 --> n3"));
}

#[tokio::test]
async fn renders_intra_clause_dependencies() {
    let source = r#"
version 1.0

task produce {
    command <<< echo value >>>
    output { String value = stdout() }
}

task consume {
    input {
        String value
    }

    command <<< echo ~{value} >>>
}

workflow test {
    if (true) {
        call produce
        call consume { input: value = produce.value }
    }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    // Inside the conditional subgraph, `produce` connects to `consume`.
    assert!(diagram.contains("subgraph n1[\"then\"]"));
    assert!(diagram.contains("n0 -->|\"yes\"| n1"));
    assert!(diagram.contains("n2 --> n3"));
}

#[tokio::test]
async fn renders_all_unconsumed_exits_from_conditional_clauses() {
    let source = r#"
version 1.0

task produce {
    command <<< echo 1 >>>

    output {
        Int x = 1
    }
}

task other {
    command <<< echo 2 >>>
}

task consume {
    input {
        Int x
    }

    command <<< echo ~{x} >>>
}

workflow test {
    input {
        Boolean cond
    }

    if (cond) {
        call produce
        call other
    }

    call consume { input: x = select_first([produce.x, 0]) }
}
"#;

    let ctx = TestContext::new(source.into());
    let diagram = ctx.render(Some(0)).await;

    // `consume` consumes `produce.x`. Both `produce` (n2) and `other` (n3) are
    // exits from the subgraph
    assert!(diagram.contains("n2 --> n4"));
    assert!(diagram.contains("n3 --> n4"));

    // Direct "no" branch from the diamond -> `consume`
    assert!(diagram.contains("n0 -->|\"no\"| n4"));
}
