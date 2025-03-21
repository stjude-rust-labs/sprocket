version 1.1

workflow test {
    output {
        Array[Object] comments = read_json("https://jsonplaceholder.typicode.com/comments?postId=1")
    }
}
