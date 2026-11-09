version 1.3

workflow main {
    input {
        Array[Int] numbers
        Boolean condition
    }

    scatter (n in numbers) {
        Int doubled = n * 2
    }

    if (condition) {
        scatter (m in numbers) {
            Int tripled = m * 3
        }
    }

    scatter (row in [[1, 2], [3, 4]]) {
        scatter (cell in row) {
            Int twice = cell * 2
        }
    }

    scatter (label in ["a", "b"]) {
        String upper = label
    }

    output {
        Array[Int] result = doubled
    }
}
