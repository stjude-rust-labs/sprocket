# WDL Lint Rules

The below rules will all be added as lints to `wdl-grammar` and `wdl-ast` (if they aren't already). Most rules will be assigned a "group" that they belong to. The intention of having these groups is twofold. One, it will allow an easy method for disabling multiple related rules at the same time. Two, it will make the documentation easier to organize and search. We're still refining what these final groups will be, so not every rule in this document will be assigned one yet. Some rules may never be assigned as a group if they are truly stand-alone. The current groups are: `spacing`, `naming`, `sorting`, `completeness`, `container`, and `deprecated`.

This document only concerns what we are calling "lint warnings". Lint warnings are distinct from what we call "validation errors". A validation error results when the specification (v1.x at time of writing) is not followed. These errors render your WDL document invalid. On the other hand, lint warnings are just our (the writers of `wdl-grammar` and `wdl-ast`) opinion on what "good" WDL should look like. They are matters of style, readability, consistency, or for encouraging proper documentation. Your document may be littered with lint warnings, but if there aren't any validation errors, then your document is valid WDL.

We are aiming to automate as much as we can to make adherence to these lint rules as easy as possible. An "auto-formatter" will be packaged as part of our [`sprocket` tool](https://github.com/stjude-rust-labs/sprocket). Auto-formatting has not yet been implemented as this project is still early in development, but while reading this document, please assume that anything easily automated (and perhaps complex to do manually) will eventually be handled by the formatter built into `sprocket`.

Some of these lint warnings are arbitrary. There are times where several different methods for styling a document could be considered equally appealing. The important part is picking one such style and adhering to it consistenly across a document or codebase. In such cases, this document will only present one option. In the future, that singular option may just be the default, with several more-or-less equivalent styles presented as alternative options available through configuration. If you feel we are picking the wrong defaults, please let us know what you think would be better!

At the time of writing (January 2024), we consider all rules herein to be up for debate. Please open an issue on this repository if you would like to suggest changes to this document.

## Rules

### `version_declaration_placement`

The WDL version declaration is required for any v1 WDL document to be parsed. A missing version declaration will result in a validation error. However an incorrect placement of the decleration will be considered a lint warning. The version decleration should be the very first line in the document, unless there are header comments. In which case, there should be a blank line between the header and the version declaration. There should _always_ be a blank line following the version declaration.

**Group**: `spacing`

#### Example

Good:

```wdl
version 1.1

...
```

Also good:

```wdl
## [Homepage](https://example.com/)

version 1.0

...
```

### `import_placement` && `import_sort`

All import statements should follow the WDL version declaration (with one empty line between the version and the first import statement).

Import statements should be sorted by the lexicographical ordering (GNU `sort` with `LC_COLLATE=C`) of each line. No extra white space is allowed between symbols or lines.

`import_placement` **group**: `spacing`

`import_sort` **group**: `sorting`

#### Example

Good:

```wdl
version 1.1

import "../../tools/fastqc.wdl" as fastqc_tasks
import "../../tools/fq.wdl"
import "./markdups-post.wdl" as markdups_post_wf
import "https://raw.githubusercontent.com/stjude/seaseq/3.0/workflows/tasks/seaseq_util.wdl" as seaseq_util
```

### `blanks_between_elements`

There should be a blank line between each WDL element at the root indentation level (such as the import block and any task/workflow definitions) and between sections of a WDL task or workflow. Never have a blank line when indentation levels are changing (such as between the opening of a workflow definition and the meta section). There should also never be blanks _within_ a meta, parameter meta, input, output, or runtime section. See example for a complete WDL document with proper spacing between elements. Note the blank lines between meta, parameter meta, input, the first call or first private declaration, output, and runtime for the example task. The blank line between the workflow definition and the task definition is also important.

**Group**: `spacing`

#### Example

Good:

```wdl
version 1.1

import "../../tools/unused-example.wdl" as unused_example

workflow hello_world {
    meta {
        description: "Greets the user of the workflow."
        output: {
            statement: "The WDL generated statement for the user.",
        }
    }

    parameter_meta {
        name: "Name to greet"
        greeting: "Phrase to use while greeting `name`"
    }

    input {
        String name = "World"
        String greeting = "Hello"
    }

    call greet { input:
        name = name,
        greeting = greeting,
    }

    output {
        String statement = greet.statement
    }
}

task greet {
    meta {
        description: "Generates a statement from a name and a greeting"
        output: {
            statement: "The result of combining the input name and greeting",
        }
    }

    parameter_meta {
        name: "Name to greet"
        greeting: "Phrase to use while greeting `name`"
    }

    input {
        String name
        String greeting
    }

    String statement = "~{greeting}, ~{name}"

    command <<<
        echo "~{statement}" > statement.txt
    >>>

    output {
        String statement = read_string("statement.txt")
    }

    runtime {
        memory: "4 GB"
        disks: "10 GB"
        container: "docker://ghcr.io/stjudecloud/util@sha256:c0583fe91d3e71fcfba58e2a57beb3420c7e907efd601f672fb5968086cd9acb"  # tag: 1.3.0
        maxRetries: 1
    }
}

```

### `inconsistent_newlines`

Files should not mix `\n` and `\r\n` line breaks. Pick one and use it consistently in your project.

### `line_width`

WDL lines should be less than or equal to 90 characters wide whenever possible. This line width restriction applies to embedded code in the `command` block. Exceptions would be long strings that WDL doesn't allow to be broken up within the meta and parameter meta sections. Another exception is `container` lines inside the `runtime` block of a task. (See the rules `mutable_container`  && `immutable_container_not_tagged` for more information about permitted `container` lines.)

### `expression_spacing`

The following rules lead to consistently readable expressions. Note that the below rules only apply to places in WDL code where an **expression** is being evaluated. There are separate rules for whitespace in other code locations.

The following tokens should be **surrounded** by whitespace when used as an infix in an expression:

* `=`
* `==`
* `!=`
* `&&`
* `||`
* `<`
* `<=`
* `>`
* `>=`
* `+`
* `-`
* `*`
* `/`
* `%`

The following tokens should **not** be **followed** by whitespace (including newlines) when used as a prefix in an expression:

* `+`
* `-`
* `!`

Opening brackets (either `(` or `[`) should **not** be **followed** by a **space** but **may** be **followed** by a **newline**.
Closing brackets (either `)` or `]`) should **not** be **preceeded** by whitespace **unless** that whitespace is **indentation**.

Sometimes a long expression will exceed the maximum line width. In these cases, one or more linebreaks must be introduced. Line continuations should be indented one more level than the beginning of the expression. There should _never_ be more than one level of indentation change per-line.

If bracketed content (things between `()` or `[]`) must be split onto multiple lines, a newline should follow the opening bracket, the contents should be indented an additional level, then the closing bracket should be de-indented to match the indentation of the opening bracket.

If you are line splitting an expression on an infix operator, the operator and at least the beginning of the RHS operand should be on the continued line. (i.e. an operator should _not_ be on a line by itself.)

If you are using the `if...then...else...` construct as part of your expression and it needs to be line split, the entire construct should be wrapped in parentheses (`()`). The opening parenthesis should be immediately followed by a newline. `if`, `then`, and `else` should all start a line one more level of indentation than the wrapping paratheses. The closing parenthesis should be on the same level of indentation as the opening parenthesis.

If you are using the `if...then...else...` construct on one line, it does not need to be wrapped in parentheses. However, if any of the 3 clauses are more complex than a single identifier, they should be wrapped in parentheses.

Sometimes a developer will choose to line split an expression despite it being able to all fit on one line that is <=90 characters wide. That is perfectly acceptable, though you may notice in the below examples the single line form can be more readable. There is "wiggle" room allowed by the above rules. This is intentional, and allows developers to choose a more compact or a more spaced out expression.

**Group**: `spacing`

#### Example

Complex example that fits on one line:

```wdl
Int complex_value = w - x + (y * (z / (foo % bar)))
```

Same example with as much line splitting as permissible:

```wdl
Int complex_value
    = w
    - x
    + (
        y
        * (
            z
            / (
                foo
                % bar
            )
        )
    )
```

Another complex example written as compactly as permissible:

```wdl
Boolean complicated_logic = (
    if !(
        a && b || c && (!d || !e)
        == (
            if foobar
            then gak
            else caz
        )
    )
    then "wow"
    else "WOWOWOW"
)
```

The same expression with as many newlines inserted as permissible:

```wdl
Boolean complicated_logic
    = (
        if
            !(
                a
                && b
                || c
                && (
                    !d
                    || !e
                )
                == (
                    if
                        foobar
                    then
                        gak
                    else
                        caz
                )
            )
        then
            "wow"
        else
            "WOWOWOW"
    )
```

### `input_spacing`

When making calls from a workflow, it is more readable and easier to edit if the supplied inputs are each on their own line. This does inflate the line count of a WDL dcoument, but it is worth it for the consistent readability. An exception _can_ be made (but does not have to be made), for calls with only a single parameter. In those cases, it is permissable to keep the input on the same line as the call.

When newline separating inputs to a call, always follow each input with a trailing comma. See examples below.

The assignment operator (`=`) should always be surrounded by whitespace.

The `input:` token should always be on the same line and separated by one space from the opening curly bracket (`{`), likeso: `call foo_task { input:`. Never newline-separate the opening bracket (`{`) from the `input:` token.

If there is a singular input and it can fit on that same line while being equal to or under the 90 character width limit, all tokens should be separated by a single space, likeso: `call foo_task { input: bam = sorted_bam }`. Note that there is no trailing comma here. This could optionally be split into 3 lines, likeso:

```wdl
call foo_task { input:
    bam = sorted_bam,
}
```

Note the trailing comma.

**Group**: `spacing`

#### Example

Good (note that these examples take advantage of the input simplifying mechanism made available in the [v1.1 specification](https://github.com/openwdl/wdl/blob/main/versions/1.1/SPEC.md#call-statement)):

```wdl
call md5sum.compute_checksum { input: file = bam }

call samtools.quickcheck { input: bam }

call util.compression_integrity { input:
    bgzipped_file = bam,
}

call markdups_post_wf.markdups_post { input:
    markdups_bam = select_first([
        markdups.duplicate_marked_bam,
        "undefined",
    ]),
    markdups_bam_index = select_first([
        markdups.duplicate_marked_bam_index,
        "undefined",
    ]),
    coverage_beds,
    coverage_labels = parse_input.labels,
    prefix = post_subsample_prefix + ".MarkDuplicates",
}
```

### `snake_case`

Variables, tasks, and workflows should be in lowercase ["snake_case"](https://en.wikipedia.org/wiki/Snake_case).

**Group**: `naming`

### `pascal_case`

Declared structs should be in ["PascalCase"](https://www.theserverside.com/definition/Pascal-case).

**Group**: `naming`

### `indentation`

Indentation should be 4 spaces and never tab literals.

### `unwanted_whitespace`

No whitespace on empty lines. No whitespace at the end of lines.

### `one_empty_line`

At most one empty line in a row. There should never be 2 or more blank lines in a row.

**Group**: `spacing`

### `newline_eof`

The file must end with a newline.

**Group**: `spacing`

### `comment_whitespace`

Comments on the same line as code should have 2 spaces before the `#` and one space before the comment text. Comments on their own line should match the indentation level around them and have one space between the `#` and the comment text. Keep in mind that even comments must be kept below the 90 character width limit.

**Group**: `spacing`

#### Example

Good:

```wdl
    input {
        Int x  # TODO better names here?
        Int y
    }

    # Next do some math
    Int z = x + y  # simple addition
```

Bad:

```wdl
    input {
        Int x # Only one space before the comment. There should be two.
        Int y  #There should be a space following the pound sign before the text.
    }

# This comment has an incorrect level of indentation
    Int z = x + y# NO spaces between code and comment is hard to read.
```

### `double_quotes`

All quotes should be double quotes.

**Group**: `naming`

#### Example

Good:

```wdl
    output {
        File md5sum = "path/to/file.txt"
        String example = "Hello, I am a \"double quoted\" string!"
    }

    runtime {
        memory: "4 GB"
        disks: "~{disk_size_gb} GB"
        container: "docker://ghcr.io/stjudecloud/util@sha256:c0583fe91d3e71fcfba58e2a57beb3420c7e907efd601f672fb5968086cd9acb"  # tag: 1.3.0
        maxRetries: 1
    }
```

Bad:

```wdl
    output {
        File md5sum = 'path/to/file.txt'
    }

    runtime {
        memory: '4 GB'
        disks: '~{disk_size_gb} GB'
        container: 'docker://ghcr.io/stjudecloud/util@sha256:c0583fe91d3e71fcfba58e2a57beb3420c7e907efd601f672fb5968086cd9acb'  # tag: 1.3.0
        maxRetries: 1
    }
```

### `trailing_comma`

All items in a comma-delimited object or list should be followed by a comma, including the last item. An exception is made for lists for which all items are on the same line, in which case there should not be a trailing comma following the last item. Note that single-line lists are _not_ allowed in the `meta` or `parameter_meta` sections. See rule `key_value_pairs` for more information.

#### Example

Good:

```wdl
   parameter_meta {
        bam: "Input BAM format file to generate coverage for"
        gtf: "Input genomic features in gzipped GTF format to count reads for"
        strandedness: {
            description: "Strandedness protocol of the RNA-Seq experiment",
            external_help: "https://htseq.readthedocs.io/en/latest/htseqcount.html#cmdoption-htseq-count-s",
            choices: [
                "yes",
                "reverse",
                "no",
            ],
        }
...
        minaqual: {
            description: "Skip all reads with alignment quality lower than the given minimum value",
            common: true,
        }
        modify_memory_gb: "Add to or subtract from dynamic memory allocation. Default memory is determined by the size of the inputs. Specified in GB."
        modify_disk_size_gb: "Add to or subtract from dynamic disk space allocation. Default disk size is determined by the size of the inputs. Specified in GB."
   }
```

Bad:

```wdl
   parameter_meta {
        bam: "Input BAM format file to generate coverage for"
        gtf: "Input genomic features in gzipped GTF format to count reads for"
        strandedness: {
            description: "Strandedness protocol of the RNA-Seq experiment",
            external_help: "https://htseq.readthedocs.io/en/latest/htseqcount.html#cmdoption-htseq-count-s",
            choices: [
                "yes",
                "reverse",
                "no"
            ]
        }
...
        minaqual: {
            description: "Skip all reads with alignment quality lower than the given minimum value",
            common: true
        }
        modify_memory_gb: "Add to or subtract from dynamic memory allocation. Default memory is determined by the size of the inputs. Specified in GB."
        modify_disk_size_gb: "Add to or subtract from dynamic disk space allocation. Default disk size is determined by the size of the inputs. Specified in GB."
   }
```

### `key_value_pairs`

All lists and objects in the `meta` and `parameter_meta` sections should have one element per line (i.e. newline separate elements). A key/value pair are considered one element **if** the value is atomic (i.e. not a list or an object). Otherwise have the key and opening bracket on the same line; subsequently indent one level; put one value per line; and have the closing bracket on its own line at the same indentation level of the key.

Lines with string values in the meta and parameter meta section are allowed to surpass the 90 character line width rule.

#### Example

Good:

```wdl
   parameter_meta {
        bam: "Input BAM format file to generate coverage for"
        gtf: "Input genomic features in gzipped GTF format to count reads for"
        strandedness: {
            description: "Strandedness protocol of the RNA-Seq experiment",
            external_help: "https://htseq.readthedocs.io/en/latest/htseqcount.html#cmdoption-htseq-count-s",
            choices: [
                "yes",
                "reverse",
                "no",
            ],
        }
...
        minaqual: {
            description: "Skip all reads with alignment quality lower than the given minimum value",
            common: true,
        }
        modify_memory_gb: "Add to or subtract from dynamic memory allocation. Default memory is determined by the size of the inputs. Specified in GB."
        modify_disk_size_gb: "Add to or subtract from dynamic disk space allocation. Default disk size is determined by the size of the inputs. Specified in GB."
   }
```

Bad:

```wdl
   parameter_meta {
        bam: "Input BAM format file to generate coverage for"
        gtf: "Input genomic features in gzipped GTF format to count reads for"
        strandedness: {
            description: "Strandedness protocol of the RNA-Seq experiment",
            external_help: "https://htseq.readthedocs.io/en/latest/htseqcount.html#cmdoption-htseq-count-s",
            choices: ["yes", "reverse", "no"]
        }
...
        minaqual: {description: "Skip all reads with alignment quality lower than the given minimum value", common: true}
        modify_memory_gb: "Add to or subtract from dynamic memory allocation. Default memory is determined by the size of the inputs. Specified in GB."
        modify_disk_size_gb: "Add to or subtract from dynamic disk space allocation. Default disk size is determined by the size of the inputs. Specified in GB."
   }
```

### `section_missing` && `section_order`

For workflows, the following sections must be present and in this order: `meta`, `parameter_meta`, `input`, (body), `output`. "(body)" represents **all** calls and declarations.

For tasks, the following sections must be present and in this order: `meta`, `parameter_meta`, `input`, (private declarations), `command`, `output`, `runtime`

`section_missing` **group**: `completeness`

`section_order` **group**: `sorting`

### `description_missing`

The meta section should have a `description` of the task or workflow.

The contents of the `description` will not be checked by the linter. However, we do have unenforced recommendations for what we believe makes a good description. It should be in active voice, beginning the first sentence with a verb. Each task/workflow is doing something. The first sentence should be a succinct description of what that "something" is. Feel free to use more than one sentence to describe your tasks and workflows. If you would rather keep your `description` entry succinct, you may write a more detailed entry under the `help` key. Additional and arbitrary `meta` entries are permitted (including `external_help`, `author`, and `email` keys).

**Group**: `completeness`

#### Example

Good:

```wdl
    meta {
        description: "Exemplifies the proper grammar for a description string."
    }
```

### `nonmatching_output`

The `meta` section should have an `output` key and keys with descriptions for each output of the task/workflow. These must match exactly. i.e. for each named output of a task or workflow, there should be an entry under `meta.output` with that same name. Additionally, these entries should be in the same order (that order is up to the developer to decide). No extraneous output entries are allowed. There should not be any blank lines inside the entire `meta` section.

**Group**: `completeness`

#### Example

Good:

```wdl
task alignment {
    meta {
        description: "Runs the STAR aligner on a set of RNA-Seq FASTQ files"
        external_help: "https://github.com/alexdobin/STAR/blob/2.7.10a/doc/STARmanual.pdf"
        output: {
            star_log: "Summary mapping statistics after mapping job is complete. The statistics are calculated for each read (single- or paired-end) and then summed or averaged over all reads. Note that STAR counts a paired-end read as one read. Most of the information is collected about the UNIQUE mappers. Each splicing is counted in the numbers of splices, which would correspond to summing the counts in SJ.out.tab. The mismatch/indel error rates are calculated on a per base basis, i.e. as total number of mismatches/indels in all unique mappers divided by the total number of mapped bases.",
            star_bam: "STAR aligned BAM",
            star_junctions: "File contains high confidence collapsed splice junctions in tab-delimited format. Note that STAR defines the junction start/end as intronic bases, while many other software define them as exonic bases. See `meta.external_help` for file specification.",
            star_chimeric_junctions: "Tab delimited file containing chimeric reads and associated metadata. See `meta.external_help` for file specification.",
        }
    }
    ...
    output {
        File star_log = prefix + ".Log.final.out"
        File star_bam = prefix + ".Aligned.out.bam"
        File star_junctions = prefix + ".SJ.out.tab"
        File? star_chimeric_junctions = prefix + ".Chimeric.out.junction"
    }
    ...
}
```

### `nonmatching_parameter_meta` && `input_not_sorted`

All inputs must have a corresponding parameter meta entry. No extraneous parameter meta entries are allowed. Inputs and parameter meta must be in the same order. No blank lines are allowed within either the input or parameter_meta blocks.

There are 2 levels of ordering for the input sections.

The high level ordering must be in this order:

1) required inputs
2) optional inputs without defaults
3) optional inputs with defaults
4) non-optional inputs with a default value

Within each of the above 3 sections, follow this sort order based on variable type:

1) `File`
2) `Array[*]+`
3) `Array[*]`
4) `struct`
5) `Object`
6) `Map[*, *]`
7) `Pair[*, *]`
8) `String`
9) `Boolean`
10) `Float`
11) `Int`

For ordering of the same compound type (`Array[*]`, `struct`, `Map[*, *]`, `Pair[*, *]`), drop the outermost type (`Array`, `Map`, etc.) and recursively apply above sorting on the _first_ inner type `*`, with ties broken by the _second_ inner type. Continue this pattern as far as possible. Once this ordering is satisfied, it is up to the developer for final order of inputs of the same type.

Does this sort order seem complex and hard to follow? Have no fear! You can rely on `sprocket`'s auto-formatting capabilities to handle this for you! (At the time of writing, auto-formatting has not yet been implemented. But it's on the docket for `sprocket`.)

`nonmatching_parameter_meta` **group**: `completeness`

`inputs_not_sorted` **group**: `sorting`

### Example

Here is a complex set of inputs in the proper sort order. The corresponding `parameter_meta` has been ommitted due it's great length, but it follows the same order. The full example can be seen [here](https://github.com/stjudecloud/workflows/blob/main/tools/star.wdl). Note that the linked `stjudecloud/workflows` repository is not fully compliant with this document.

```wdl
    input {
        File star_db_tar_gz
        Array[File] read_one_fastqs_gz
        String prefix
        String? read_groups
        Array[File] read_two_fastqs_gz = []
        Array[Int] out_SJ_filter_intron_max_vs_read_n = [50000, 100000, 200000]
        SJ_Motifs out_SJ_filter_overhang_min = SJ_Motifs {
            noncanonical_motifs: 30,
            GT_AG_and_CT_AC_motif: 12,
            GC_AG_and_CT_GC_motif: 12,
            AT_AC_and_GT_AT_motif: 12
        }
        SJ_Motifs out_SJ_filter_count_unique_min = SJ_Motifs {
            noncanonical_motifs: 3,
            GT_AG_and_CT_AC_motif: 1,
            GC_AG_and_CT_GC_motif: 1,
            AT_AC_and_GT_AT_motif: 1
        }
        SJ_Motifs out_SJ_filter_count_total_min = SJ_Motifs {
            noncanonical_motifs: 3,
            GT_AG_and_CT_AC_motif: 1,
            GC_AG_and_CT_GC_motif: 1,
            AT_AC_and_GT_AT_motif: 1
        }
        SJ_Motifs out_SJ_filter_dist_to_other_SJ_min = SJ_Motifs {
            noncanonical_motifs: 10,
            GT_AG_and_CT_AC_motif: 0,
            GC_AG_and_CT_GC_motif: 5,
            AT_AC_and_GT_AT_motif: 10
        }
        SJ_Motifs align_SJ_stitch_mismatch_n_max = SJ_Motifs {
            noncanonical_motifs: 0,
            GT_AG_and_CT_AC_motif: -1,
            GC_AG_and_CT_GC_motif: 0,
            AT_AC_and_GT_AT_motif: 0
        }
        Pair[String, String] clip_3p_adapter_seq = ("None", "None")
        Pair[Float, Float] clip_3p_adapter_MMp = (0.1, 0.1)
        Pair[Int, String] align_ends_protrude = (0, "ConcordantPair")
        Pair[Int, Int] clip_3p_n_bases = (0, 0)
        Pair[Int, Int] clip_3p_after_adapter_n_bases = (0, 0)
        Pair[Int, Int] clip_5p_n_bases = (0, 0)
        String read_name_separator = "/"
        String clip_adapter_type = "Hamming"
        String out_SAM_strand_field = "intronMotif"
        String out_SAM_attributes = "NH HI AS nM NM MD XS"
        String out_SAM_unmapped = "Within"
        String out_SAM_order = "Paired"
        String out_SAM_read_ID = "Standard"
        String out_SAM_tlen = "left_plus"
        String out_filter_type = "Normal"
        String out_filter_intron_motifs = "None"
        String out_filter_intron_strands = "RemoveInconsistentStrands"
        String out_SJ_filter_reads = "All"
        String align_ends_type = "Local"
        String align_soft_clip_at_reference_ends = "Yes"
        String align_insertion_flush = "None"
        String chim_out_type = "Junctions"
        String chim_filter = "banGenomicN"
        String chim_out_junction_format = "plain"
        String twopass_mode = "Basic"
        Boolean use_all_cores = false
        Float out_filter_mismatch_n_over_L_max = 0.3
        Float out_filter_mismatch_n_over_read_L_max = 1.0
        Float out_filter_score_min_over_L_read = 0.66
        Float out_filter_match_n_min_over_L_read = 0.66
        Float score_genomic_length_log2_scale = -0.25
        Float seed_search_start_L_max_over_L_read = 1.0
        Float align_spliced_mate_map_L_min_over_L_mate = 0.66
        Float pe_overlap_MMp = 0.01
        Int run_RNG_seed = 777
        Int sjdb_score = 2
        Int read_map_number = -1
        Int read_quality_score_base = 33
        Int limit_out_SJ_one_read = 1000
        Int limit_out_SJ_collapsed = 1000000
        Int limit_sjdb_insert_n_sj = 1000000
        Int out_QS_conversion_add = 0
        Int out_SAM_attr_IH_start = 1
        Int out_SAM_mapq_unique = 255
        Int out_SAM_flag_OR = 0
        Int out_SAM_flag_AND = 65535
        Int out_filter_multimap_score_range = 1
        Int out_filter_multimap_n_max = 20
        Int out_filter_mismatch_n_max = 10
        Int out_filter_score_min = 0
        Int out_filter_match_n_min = 0
        Int score_gap = 0
        Int score_gap_noncan = -8
        Int score_gap_GCAG = -4
        Int score_gap_ATAC = -8
        Int score_del_open = -2
        Int score_del_base = -2
        Int score_ins_open = -2
        Int score_ins_base = -2
        Int score_stitch_SJ_shift = 1
        Int seed_search_start_L_max = 50
        Int seed_search_L_max = 0
        Int seed_multimap_n_max = 10000
        Int seed_per_read_n_max = 1000
        Int seed_per_window_n_max = 50
        Int seed_none_loci_per_window = 10
        Int seed_split_min = 12
        Int seed_map_min = 5
        Int align_intron_min = 21
        Int align_intron_max = 500000
        Int align_mates_gap_max = 1000000
        Int align_SJ_overhang_min = 5
        Int align_sjdb_overhang_min = 1
        Int align_spliced_mate_map_L_min = 0
        Int align_windows_per_read_n_max = 10000
        Int align_transcripts_per_window_n_max = 100
        Int align_transcripts_per_read_n_max = 10000
        Int pe_overlap_n_bases_min = 0
        Int win_anchor_multimap_n_max = 50
        Int win_bin_n_bits = 16
        Int win_anchor_dist_n_bins = 9
        Int win_flank_n_bins = 4
        Int chim_segment_min = 0
        Int chim_score_min = 0
        Int chim_score_drop_max = 20
        Int chim_score_separation = 10
        Int chim_score_junction_non_GTAG = -1
        Int chim_junction_overhang_min = 20
        Int chim_segment_read_gap_max = 0
        Int chim_main_degment_mult_n_max = 10
        Int chim_multimap_n_max = 0
        Int chim_multimap_score_range = 1
        Int chim_non_chim_score_drop_min = 20
        Int twopass1_reads_n = -1
        Int ncpu = 8
        Int modify_disk_size_gb = 0
    }
```

### `disallowed_input_name`

Any input name matching these regular expressions will be flagged: `/^[iI]n[A-Z_]/` or `/^input/i`. It is redundant and needlessly verbose to use an input's name to specify that it is an input. Input names should be short yet descriptive. Prefixing a name with `in` or `input` adds length to the name without adding clarity or context.

**Group**: `naming`

#### Example

All the below input names would be flagged:

```wdl
    input {
        File inBam  # This name will be flagged twice, once for the 'in' prefix, and once for being camalCase when it should be snake_case
        File in_bam
        File input_gtf
    }
```

### `disallowed_output_name`

Any output name matching these regular expressions will be flagged: `/^[oO]ut[A-Z_]/`, `/^output/i` or `/^..?$/`. It is redundant and needlessly verbose to use an output's name to specify that it is an output. Output names should be short yet descriptive. Prefixing a name with `out` or `output` adds length to the name without adding clarity or context. Additionally, names with only 2 characters can lead to confusion and obfuscates the content of an output. Output names should be at least 3 characters long.

**Group**: `naming`

#### Example

All the below output names would be flagged:

```wdl
    input {
        File outBam  # This name will be flagged twice, once for the 'out' prefix, and once for being camalCase when it should be snake_case
        File out_bam
        File output_gtf
        String af
    }
```

### `no_curly_commands`

`command` blocks should be wrapped with arrows (`<<<` `>>>`) instead of curly brackets (`{` `}`). Certain Bash constructions cause problems with parsing the bracket notation. There are no such problems with the arrow notation.

#### Example

Good:

```wdl
    command <<<
        echo "Hello, World!"
    >>>
```

Bad:

```wdl
    command {
        echo "Hello, World!"
    }
```

### `mutable_container`  && `immutable_container_not_tagged`

All tasks should run in an immutable container. This ensures reproducibility across time and environments. `wdl-grammar` and `wdl-ast` will look for a `:SHASUM` tag in your `container` declarations and warn if one is missing. A `sha` digest will always point to the exact same image, whereas tags are mutable. This mutability makes even versioned tags problematic when we want to ensure reproducibility.

While the confidence in persistence gained by using `sha` digests for pulling container images is valuable, it comes at a cost: lack of human readability. So we enforce following `container` strings with a comment which gives a human readable name to the image being pulled. It is imperative that the `sha` digest and the human readable tag are kept in sync. It's extra overhead compared to just using a tag directly, but we consider it well worth it.

Note that `container` lines are permitted to exceed the 90 character width limit.

**Group**: `container`

#### Example

Bad:

```wdl
    runtime {
        memory: "4 GB"
        disks: "10 GB"
        container: "docker://quay.io/biocontainers/htseq:2.0.4--py310h5aa3a86_0"
        maxRetries: 1
    }
```

Good:

```wdl
    runtime {
        memory: "4 GB"
        disks: "10 GB"
        container: "docker://quay.io/biocontainers/htseq@sha256:04309f74909f7e48bc797ee5faa4e4388d7f581890c092a455d15bbcf5f6c537"  # tag: 2.0.4
        maxRetries: 1
    }
```

### `file_coercion`

String-to-File coercions should only be used in task output. Anywhere else will be flagged. Hardcoding filepaths like this might work in your local environment, but this is not a portable solution and should be discouraged.

#### Example

Good:

```wdl
    output {
        File processed_file = "path/within/running/container/file.txt"
    }
```

Bad:

```wdl
    input {
        File bam
        File ref_fasta = "/path/on/your/local/compute/environment/GRCh38_no_alt.fa"
    }
```

### `runtime_failable_optional_coercion`

It is always allowed by the specification to coerce an optional type into a required type. However, these coercions may fail at runtime. `wdl-grammar` and `wdl-ast` will warn if it's possible for your coercions to fail at runtime. There are a number of ways to prevent runtime failable coercions, and this document will not go through them all. However some of the common methods for preventing these failures are by wrapping the coercion inside an `if (defined(X))` block or eliminating the coercion completely with a `select_first([])` statement.

#### Example

Bad:

```wdl
workflow hello_world {
    meta {
        description: "Greets the user of the workflow."
        output: {
            statement: "The WDL generated statement for the user.",
        }
    }

    parameter_meta {
        greeting: "Phrase to use while greeting `name`"
        name: "Name to greet"
    }

    input {
        String greeting = "Hello"
        String? name
    }

    call greet { input:
        name,  # This will fail if the user doesn't provide a `name`
        greeting,
    }

    output {
        String statement = greet.statement
    }
}

task greet {
    meta {
        description: "Generates a statement from a name and a greeting"
        output: {
            statement: "The result of combining the input name and greeting",
        }
    }

    parameter_meta {
        name: "Name to greet"
        greeting: "Phrase to use while greeting `name`"
    }

    input {
        String name
        String greeting
    }

    String statement = "~{greeting}, ~{name}"

    command <<<
        echo "~{statement}" > statement.txt
    >>>

    output {
        statement = read_string("statement.txt")
    }

    runtime {
        memory: "4 GB"
        disks: "10 GB"
        container: "docker://ghcr.io/stjudecloud/util@sha256:c0583fe91d3e71fcfba58e2a57beb3420c7e907efd601f672fb5968086cd9acb"  # tag: 1.3.0
        maxRetries: 1
    }
}

```

Better:

```wdl
workflow hello_world {
    meta {
        description: "Greets the user of the workflow."
        output: {
            statement: "The WDL generated statement for the user.",
        }
    }

    parameter_meta {
        greeting: "Phrase to use while greeting `name`"
        name: "Name to greet"
    }

    input {
        String greeting = "Hello"
        String? name
    }

    if (defined(name)) {
        call greet { input:
            name,
            greeting,
        }
    }
    if (!defined(name)) {
        call greet_anonymous { input: greeting }
    }

    output {
        String statement = select_first([greet.statement, greet_anonymous.statement])
    }
}

task greet {
    meta {
        description: "Generates a statement from a name and a greeting"
        output: {
            statement: "The result of combining the input name and greeting",
        }
    }

    parameter_meta {
        name: "Name to greet"
        greeting: "Phrase to use while greeting `name`"
    }

    input {
        String name
        String greeting
    }

    String statement = "~{greeting}, ~{name}"

    command <<<
        echo "~{statement}" > statement.txt
    >>>

    output {
        statement = read_string("statement.txt")
    }

    runtime {
        memory: "4 GB"
        disks: "10 GB"
        container: "docker://ghcr.io/stjudecloud/util@sha256:c0583fe91d3e71fcfba58e2a57beb3420c7e907efd601f672fb5968086cd9acb"  # tag: 1.3.0
        maxRetries: 1
    }
}

task greet_anonymous {
    meta {
        description: "Generates a statement from a greeting"
        output: {
            statement: "The greeting"
        }
    }

    parameter_meta {
        greeting: "Phrase to use while greeting"
    }

    input {
        String greeting
    }

    String statement = "~{greeting}"

    command <<<
        echo "~{statement}!" > statement.txt
    >>>

    output {
        statement = read_string("statement.txt")
    }

    runtime {
        memory: "4 GB"
        disks: "10 GB"
        container: "docker://ghcr.io/stjudecloud/util@sha256:c0583fe91d3e71fcfba58e2a57beb3420c7e907efd601f672fb5968086cd9acb"  # tag: 1.3.0
        maxRetries: 1
    }
}
```

Although the above solves the lint warning issue, it's not very well written. It was a contrived example to illustrate one way to silence the lint warning (wrap the coercion in an `if (defined(<>))` block). We will leave it as an exercise for the reader what the "Best" option might be. (there are at least 2 ways: either rewrite the `greet` task to accept an optional `name`, or use the standard library's `select_first()` function with a default name. Either implementation will eliminate the lint warning).

### `runtime_failable_nonempty_coercion`

This rule is very similar `runtime_failable_optional_coercion`. It concerns coercing a potentially empty array (`Array[*]`) into an non-empty array `Array[*]+`. This may fail at runtime depending on user input. Much like `runtime_failable_optional_coercion`, we will not exhaustively cover solutions at this time (maybe in a future version of this document?).

### `incomplete_call`

A workflow must provide _every_ required input to a task. Technically, this is not a requirement of WDL which is why this rule is a lint warning instead of a validation error. The required inputs could be supplied in the input JSON with the following syntax: `workflow_name.task_name.required_input_name: <value>`. However this is confusing and error-prone when reading the workflow. _All_ required inputs should be declared and supplied in the top-level workflow. Even if that input is only being used in one call. This lint warning will only trigger **if** the workflow has declared `allowNestedInputs: true` in the `meta` block. If this declaration is not there, according to the specification the above method of supplying inputs to nested calls is disbarred. If that is the case, `wdl-grammar` and `wdl-ast` will throw a validation error instead of a lint warning.

### `name_collision`

Name collisions of the same kind will result in a validation error, as it will be ambiguous to the execution engine which instance of the name you are referring to. However, name collisions between declarations of _different_ kinds, such as a workflow name and a struct type, can be parsed and will not be a validation error. Instead they will fall under this lint warning, as they can be very confusing to readers (if not execution engines).

#### Example

Bad:

```wdl
workflow foo {
    ...

    input {
        String foo
    }

    call foo { input: foo = foo }

    ...
}

task foo {
    ...

    input {
        String foo
    }

    ...
}
```

The above example may be difficult to read due to it's repeated use of the name `foo`, but it is perfectly parseable WDL that wouldn't cause an issue for an execution engine. To the engine, it is perfectly clear which instance of `foo` is being referred to in each place. However, if you use `wdl-grammar` and `wdl-ast`, you will get multiple lint warnings over the name collisions on `foo`.

The below example disambiguates the workflow from the task by adding suffixes (`_wf` and `_task` respectively). It keeps `foo` as the name for the two inputs and takes advantage of a mechanism added by the v1.1 specification that allows us to drop the `<input>=` if the task input (`foo`) and the declaration in the current scope (`foo`) share the same name. (See the ["Call Statement" section](https://github.com/openwdl/wdl/blob/main/versions/1.1/SPEC.md#call-statement) for more information about this mechanism.)

```wdl
workflow foo_wf {
    ...

    input {
        String foo
    }

    call foo_task { input: foo }

    ...
}

task foo_task {
    ...

    input {
        String foo
    }

    ...
}
```

### `unused_import`

An import which is not used anywhere in the document. While relatively harmless, these unused imports can be confusing to readers and maintainers alike.

### `unused_declaration`

Nothing references a declaration. Much like unused imports, these can be confusing to readers and maintainers.

### `select_nonoptional_array`

It is unnecessary to use `select_first()` or `select_all()` on an array which is non-optional. This usage is confusing.

#### Example

Bad:

```wdl
    Int x = 1
    Int y = 2
    Int z = select_first([x, y])  # z will always evaluate to 1

    Array[String] foobar = ["foo", "bar",]
    Array[String] selected = select_all(foobar)  # selected will always be equivalent to foobar
```

Good:

```wdl
    Int x = 1
    Int y = 2
    Int z = x

    Array[String] foobar = ["foo", "bar",]
    Array[String] also_foobar = foobar
```

### `deprecated_unknown_runtime_key`

Arbitrary "hints" were at one point allowed in the runtime section. However this has been deprecated. Additionally, the `docker` key has been deprecated in favor of `container`. The list of reserved and allowed keys for the runtime section are:

#### Keys implemented by every execution engine

* `container`
* `cpu`
* `memory`
* `gpu`
* `disks`
* `maxRetries`
* `returnCodes`

#### "Hints" optionally implemented by execution engines**

* `maxCpu`
* `maxMemory`
* `shortTask`
* `localizationOptional`
* `inputs`
* `outputs`

**Group**: `deprecated`

### `deprecated_placeholder`

The "placeholder options" of `sep`, `true/false`, and `default` have been deprecated in favor of other methods for acheiving similar effect. See the "good" example below for the alternatives.

**Group**: `deprecated`

#### Example

Bad:

```wdl
    Array[Int] numbers = [1,2,3]
    Boolean allow_foo = true
    String? bar = None

    command <<<
        set -euo pipefail

        python script.py ~{sep=" " numbers}
        example-command ~{true="--enable-foo" false="" allow_foo}
        another-command ~{default="foobar" bar}
    >>>
```

Good:

```wdl
    Array[Int] numbers = [1,2,3]
    Boolean allow_foo = true
    String? bar = None

    command <<<
        set -euo pipefail

        python script.py ~{sep(" ", numbers)}
        example-command ~{if allow_foo then "--enable-foo" else ""}
        another-command ~{select_first([bar, "foobar"])}
        # OR also equivalent
        another-command ~{if defined(bar) then bar else "foobar"}
    >>>
```

### `deprecated_object`

The `Object` type has been deprecated in favor of using `Struct` types.

**Group**: `deprecated`

#### Example

Bad:

```wdl
workflow {
    ...

    Object literal_object = object {
        a: 10,
        b: "foo",
    }

    ...
}
```

Good:

```wdl
workflow {
    ...

    FooBar literal_struct = {
        a: 10
        b: "foo"
    }

    ...
}

# unlike objects, structs must be defined and typed
struct FooBar {
    Int a
    String b
}
```
