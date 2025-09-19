version 1.1

workflow test {
    output {
        Object todos = read_json("https://jsonplaceholder.typicode.com/todos/1")
    }
}
