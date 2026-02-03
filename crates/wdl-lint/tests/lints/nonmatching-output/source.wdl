#@ except: MetaDescription, OutputName, RuntimeSection

version 1.1

# This task is OK
task foo {
    meta {
        outputs: {
            out: "String output of task foo"
        }
    }

    command <<< >>>

    output {
        String out = read_string(stdout())
    }
}

# This task should trigger a warning for missing `meta.outputs`.
task bar {
    meta {}

    command <<< >>>

    output {
        String s = "hello"
    }
}

# This task should trigger a warning for `t` missing in `meta.outputs`.
task baz {
    meta {
        outputs: {
            s: "String output of task baz"
        }
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
    }
}

# This task should trigger a warning for `meta.outputs` being out-of-order.
task qux {
    meta {
        outputs: {
            t: "t",
            s: "s",
        }
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
    }
}

# This task should trigger a warning for the extra `v` in `meta.outputs`.
task quux {
    meta {
        outputs: {
            s: "s",
            t: "t",
            v: "v",
        }
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
    }
}

# This task should trigger a warning for `outputs` being non-object.
# Also warnings for `s`, `t`, and `v` not in `meta.outputs`.
task corge {
    meta {
        outputs: "string"
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
        String v = "!"
    }
}

# This task should not trigger any warnings.
task grault {
    meta {}

    command <<< >>>

    output {}  # There should be no warnings here.
}

task garply {
    meta {
        outputs: {
            s: "s",
            t: "t",
            # The next lint directive will _not_ work.
            #@ except: MatchingOutputMeta
            v: "v",
        }
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
    }
}

# This task should not trigger a warning due to `#@ except`.
#@ except: MatchingOutputMeta
task garply2 {
    meta {
        outputs: {
            s: "s",
            t: "t",
            v: "v",
        }
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
    }
}

#@ except: MatchingOutputMeta
# This task should not trigger a warning due to `#@ except`.
task waldo {
    meta {
        outputs: {
            s: "s",
            t: "t",
        }
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
        String v = "!"
    }
}

# This should not trigger any warnings.
#@ except: MatchingOutputMeta
task waldo2 {
    meta {
        outputs: {
            s: "s",
            t: "t",
        }
    }

    command <<< >>>

    output {
        String s = "hello"
        String t = "world"
        String v = "!"
    }
}

# This should trigger a warning to the extra `s`, `t`, and `v` in `meta.outputs`.
task quuux {
    meta {
        outputs: {
            s: "s",
            t: "t",
            v: "v",
        }
    }

    command <<< >>>

    output {}
}

# This should not trigger a warning.
task quuuux {
    meta {
        outputs: {
            # another comment
            s: {
                # adding a comment
                description: "s",
            },
        }
    }

    command <<< >>>

    output {
        String s = "string"
    }
}

# This task has meta.outputs but NO output section at all.
# All entries in meta.outputs should be flagged as extra.
task no_output_section {
    meta {
        outputs: {
            nonexistent: "This output does not exist",
            also_nonexistent: "Neither does this one",
        }
    }

    command <<< >>>
}
