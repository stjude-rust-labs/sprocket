version 1.3

## This is correct - doc comment before directive
#@ except: Foo
workflow correct_order {
    meta {}
}

#@ except: Bar
## This is incorrect - doc comment after directive
task incorrect_order {
    command <<<>>>
}

## Multiple doc comments
## should be detected together
#@ except: Baz
struct incorrect_struct {
    String foo
}

#@ except: Qux
#@ except: Quux
## Doc comment after multiple directives
enum incorrect_enum {
    A,
    B,
}

## Doc before directive
#@ except: LineWidth
## Another doc comment after directive
task mixed_order {
    command <<<>>>
}

## Just doc comments, no directives
task no_directives {
    command <<<>>>
}

## Doc before directive
#@ except: SomeRule

## Another doc block after blank line and directive
struct with_blank_lines {
    String bar
}
