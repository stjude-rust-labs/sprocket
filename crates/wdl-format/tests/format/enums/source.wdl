version 1.3

enum Color {
    Red,
    Green
    Blue
}

enum Status[String]
{
    Pending,    Running,
    Complete}

enum Priority[Int] {
    Low = 1,
    Medium = 2
    High = 3,
}

workflow test {
    Status s = Status.Pending
}
