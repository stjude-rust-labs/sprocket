version 1.3

struct BankAccount {
  String account_number
  Int routing_number
  Float balance
  Array[Int]+ pin_digits
  String? username
}

struct Person {
  String name
  BankAccount? account
}

task test_struct {
  command <<<>>>

  output {
    Person john = Person {
      name: "John",
      # it's okay to leave out username since it's optional
      account: BankAccount {
        account_number: "123456",
        routing_number: 300211325,
        balance: 3.50,
        pin_digits: [1, 2, 3, 4]
      }
    }
    Boolean has_account = defined(john.account)
  }
}
