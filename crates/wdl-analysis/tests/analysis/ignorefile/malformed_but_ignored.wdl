## This should NOT result in diagnostics as the entire file should be ignored

version 1.0

task bar {
    command {
        echo "brace mismatch"
    >>>
}