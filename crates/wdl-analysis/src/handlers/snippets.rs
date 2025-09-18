//! Built-in snippets for WDL.

use std::borrow::Cow;
use std::sync::LazyLock;

use wdl_ast::SyntaxKind;

/// The snippet type used by LSP.
#[derive(Debug)]
pub struct Snippet {
    /// The label for the snippet in the completion list.
    pub label: &'static str,
    /// The snippet text to insert.
    pub insert_text: &'static str,
    /// A short description of the snippet.
    pub detail: &'static str,
    /// The AST contexts in which this snippet is applicable.
    pub contexts: &'static [SyntaxKind],
}

/// The snippets used by lsp
pub static SNIPPETS: LazyLock<Vec<Snippet>> = LazyLock::new(|| {
    vec![
        Snippet {
            label: "struct",
            insert_text: "struct ${TM_SELECTED_TEXT:${1:MyStruct}} \
                          {\n\t${2|Array,Boolean,Directory,File,Float,Int,Map,Object,Pair,\
                          String|} ${3:name}\n}",
            detail: "Create a new struct",
            contexts: &[SyntaxKind::RootNode],
        },
        Snippet {
            label: "task",
            insert_text: "task ${TM_SELECTED_TEXT:${1:my_task}} {\n\tcommand <<<\n\t\techo \
                          \"Hello, world!\"\n\t>>>\n\n\trequirements {\n\t\tcontainer: \
                          \"ubuntu:latest\"\n\t}\n}",
            detail: "Create a new task",
            contexts: &[SyntaxKind::RootNode],
        },
        Snippet {
            label: "meta",
            insert_text: "meta {\n\tdescription: \"${1: This is a description.}\"\n\n}",
            detail: "Create a new `meta` section",
            contexts: &[
                SyntaxKind::TaskDefinitionNode,
                SyntaxKind::WorkflowDefinitionNode,
            ],
        },
        Snippet {
            label: "parameter_meta",
            insert_text: "parameter_meta {\n\t$0\n\n}",
            detail: "Create a new `parameter_meta` section",
            contexts: &[
                SyntaxKind::TaskDefinitionNode,
                SyntaxKind::WorkflowDefinitionNode,
                SyntaxKind::StructDefinitionNode,
            ],
        },
        Snippet {
            label: "input",
            insert_text: "input {\n\t${1|Array,Boolean,Directory,File,Float,Int,Map,Object,Pair,\
                          String|} ${2:name}\n\n}",
            detail: "Create a new `input` section",
            contexts: &[
                SyntaxKind::TaskDefinitionNode,
                SyntaxKind::WorkflowDefinitionNode,
            ],
        },
        Snippet {
            label: "output",
            insert_text: "output {\n\t${1|Array,Boolean,Directory,File,Float,Int,Map,Object,Pair,\
                          String|} ${2:name} = $0\n\n}",
            detail: "Create a new `output` section",
            contexts: &[
                SyntaxKind::TaskDefinitionNode,
                SyntaxKind::WorkflowDefinitionNode,
            ],
        },
        Snippet {
            label: "requirements",
            insert_text: "requirements {\n\tcontainer: ${1:\"*\"}\n\tcpu: ${2:1}\n\tmemory: \
                          ${3:\"2 GiB\"}\n\tgpu: ${4:false}\n\tfpga: ${5:false}\n\tdisks: ${6:\"1 \
                          GiB\"}\n\tmax_retries: ${7:0}\n\treturn_codes: ${8:0}\n\n}",
            detail: "Create a new `requirements` section",
            contexts: &[SyntaxKind::TaskDefinitionNode],
        },
        Snippet {
            label: "runtime",
            insert_text: "runtime {\n\tcontainer: ${1:\"*\"}\n\tcpu: ${2:1}\n\tmemory: ${3:\"2 \
                          GiB\"}\n\tdisks: ${4:\"1 GiB\"}\n\tgpu: ${5:false}\n\n}",
            detail: "Create a new `runtime` section",
            contexts: &[SyntaxKind::TaskDefinitionNode],
        },
        Snippet {
            label: "hints",
            insert_text: "hints {\n\tmax_cpu: ${1:32}\n\tmax_memory: ${2:\"32 GiB\"}\n\tdisks: \
                          ${3:\"500 GiB\"}\n\tgpu: ${4:0}\n\tfpga: ${5:0}\n\tshort_task: \
                          ${6:false}\n\tlocalization_optional: ${7:false}\n\t# inputs: TODO \
                          (e.g., `input { name: hints { min_length: 3 } }`)\n\t# outputs: TODO \
                          (e.g., `output { name: hints { max_length: 5 } }`)\n\n}",
            detail: "Create a new `hints` section",
            contexts: &[SyntaxKind::TaskDefinitionNode],
        },
        Snippet {
            label: "full-task",
            insert_text: r#"task ${TM_SELECTED_TEXT:${1:my_task}} {
	meta {
		description: "${2:This task greets the name passed to the input.}"
	}

	parameter_meta {
		name: "${3:The name to say 'hello' to.}"
	}

	input {
		String name
	}

	command <<<
		echo "Hello, ~{name}"
	>>>

	output {
		$0
	}

	requirements {
		container: "*"
		cpu: 1
		memory: "2 GiB"
		gpu: false
		fpga: false
		disks: "1 GiB"
		max_retries: 0
		return_codes: 0
	}

	hints {
		max_cpu: 32
		max_memory: "32 GiB"
		disks: "500 GiB"
		gpu: 0
		fpga: 0
		short_task: false
		localization_optional: false
		# inputs: TODO (e.g., `input { name: hints { min_length: 3 } }`)
		# outputs: TODO (e.g., `output { name: hints { max_length: 5 } }`)
	}
}"#,
            detail: "Create a new complete task",
            contexts: &[SyntaxKind::RootNode],
        },
        Snippet {
            label: "call",
            insert_text: "call ${1:my_task} {\n\t$0\n}",
            detail: "Create a new `call` statement",
            contexts: &[
                SyntaxKind::WorkflowDefinitionNode,
                SyntaxKind::ScatterStatementNode,
                SyntaxKind::ConditionalStatementNode,
            ],
        },
        Snippet {
            label: "if",
            insert_text: "if (${1:condition}) {\n\t$0\n}",
            detail: "Create a new `if` statement",
            contexts: &[
                SyntaxKind::WorkflowDefinitionNode,
                SyntaxKind::ScatterStatementNode,
                SyntaxKind::ConditionalStatementNode,
            ],
        },
        Snippet {
            label: "scatter",
            insert_text: "scatter (${1:item} in ${2:items}) {\n\t$0\n}",
            detail: "Create a new `scatter` statement",
            contexts: &[
                SyntaxKind::WorkflowDefinitionNode,
                SyntaxKind::ScatterStatementNode,
                SyntaxKind::ConditionalStatementNode,
            ],
        },
        Snippet {
            label: "workflow",
            insert_text: "workflow ${TM_SELECTED_TEXT:${1:my_workflow}} {\n\tinput \
                          {\n\t\t$2\n\t}\n\n\tcall ${3:my_task} {\n\t\t$4\n\t}\n\n\n\toutput \
                          {\n\t\t$0\n\t}\n}",
            detail: "Create a new workflow",
            contexts: &[SyntaxKind::RootNode],
        },
        Snippet {
            label: "#@ except:",
            insert_text: &Cow::Borrowed("#@ except: $0"),
            detail: "Create a new except comment",
            contexts: &[
                SyntaxKind::RootNode,
                SyntaxKind::TaskDefinitionNode,
                SyntaxKind::WorkflowDefinitionNode,
                SyntaxKind::ScatterStatementNode,
                SyntaxKind::ConditionalStatementNode,
                SyntaxKind::StructDefinitionNode,
            ],
        },
    ]
});
