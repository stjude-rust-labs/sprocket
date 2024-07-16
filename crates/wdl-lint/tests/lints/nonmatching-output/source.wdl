#@ except: DescriptionMissing, MissingRuntime

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
            v: "v"
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
    output {} # There should be no warnings here.
}

# This task should not trigger a warning due to `#@ except`.
task garply {
    # except works here
    meta {
        # except doesn't work here. In fact, this triggers a missing "outputs" warning.
        outputs: {
            s: "s",
            t: "t",
            # This also works
            #@ except: NonmatchingOutput
            v: "v"
        }
    }
    command <<< >>>
    output {
        String s = "hello"
        String t = "world"
    }
}

# This task should not trigger a warning due to `#@ except`.
task garply2 {
    # except works here
    #@ except: NonmatchingOutput
    meta {
        # except doesn't work here. In fact, this triggers a missing "outputs" warning.
        outputs: {
            s: "s",
            t: "t",
            # This also works
            v: "v"
        }
    }
    command <<< >>>
    output {
        String s = "hello"
        String t = "world"
    }
}

# This task should not trigger a warning due to `#@ except`.
task waldo {
    meta {
        outputs: {
            s: "s",
            t: "t",
        }
    }
    command <<< >>>
    # except works here
    output {
        String s = "hello"
        String t = "world"
        # Also here
        #@ except: NonmatchingOutput
        String v = "!"
    }
}

# This should not trigger any warnings.
task waldo2 {
    meta {
        outputs: {
            s: "s",
            t: "t",
        }
    }
    command <<< >>>
    # except works here
    #@ except: NonmatchingOutput
    output {
        String s = "hello"
        String t = "world"
        # Also here
        String v = "!"
    }
}

# This should trigger a warning to the extra `s`, `t`, and `v` in `meta.outputs`.
task quuux {
    meta {
        outputs: {
            s: "s",
            t: "t",
            v: "v"
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
                description: "s"
            },
        }
    }
    command <<< >>>
    output {
        String s = "string"
    }
}
