module calculator_package::calculator_module {
  use std::signer;

  struct Calc has key {
    sum: u64
  }

  public entry fun fuzz_init(account: &signer) {
    move_to(account, Calc { sum: 0 })
  }

  public entry fun add(account: &signer, a: u64) acquires Calc {
    let calc = borrow_global_mut<Calc>(signer::address_of(account));
    calc.sum = calc.sum + a;
  }

  public entry fun sub(account: &signer, a: u64) acquires Calc {
    let calc = borrow_global_mut<Calc>(signer::address_of(account));
    calc.sum = calc.sum - a;
  }

}
