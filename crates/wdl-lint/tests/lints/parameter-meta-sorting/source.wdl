#@ except: MetaDescription, InputName, RequirementsSection

version 1.3

task parameter_meta_matched {
    meta {}

    parameter_meta {
        p: "Array of non-optional strings"
        w: "Directory input"
        b: "Another file input"
        q: "Another array of non-optional strings"
        t: "File input"
    }

    input {
        Directory w
        Array[String]+ p
        File t
        Array[String]+ q
        File b
    }

    command <<<>>>

    output {}
}
