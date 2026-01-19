## This is a test of leading whitespace stripping and normalization in command blocks.

version 1.3

task no_whitespace {
    command <<<
        echo "hello"
    >>>
}

task short_leading_whitespace {
    command <<<
        echo "hello"
    >>>
}

task long_leading_whitespace {
    command <<<
        echo "hello"
    >>>
}

task mixed_leading_whitespace {
    command <<<
        echo "hello"
              echo "world"
    >>>
}
