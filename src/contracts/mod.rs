pub use codegen::LnRewardSystem;

mod codegen {
    use ethers::prelude::*;

    abigen!(LnRewardSystem, "./src/contracts/abis/LnRewardSystem.json");
}
