#@ except: ShellCheck, MetaSections, MetaDescription, ExpectedRuntimeKeys, ParameterMetaMatched, MatchingOutputMeta, ParameterDescription, DescriptionLength, RuntimeSection, HereDocCommands, InputName, OutputName, SectionOrdering, ContainerUri, ConciseInput, DeclarationName, SnakeCase, UnusedDocComments, TodoComment, DoubleQuotes, ConsistentNewlines, DocCommentTabs, DocMetaStrings

## OptionalInputSafety (#715): bad task should warn; good task should not.

version 1.1

task bad_optional_in_command {
    input {
        File input_bam
        String? optional_flag
    }

    command <<<
        set -euo pipefail
        samtools sort ~{optional_flag} ~{input_bam}
    >>>

    output {
        File sorted_bam = input_bam
    }
}

task good_guarded_optional {
    input {
        File input_bam
        String? optional_flag
    }

    command <<<
        set -euo pipefail
        samtools sort ~{if defined(optional_flag) then optional_flag else ""} ~{input_bam}
    >>>

    output {
        File sorted_bam = input_bam
    }
}
