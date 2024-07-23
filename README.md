# Activate Stylus Program

Simple script in Rust to activate a Stylus program at a given address.
It estimates the required activation data fee for the program and then sends a transaction
to activate it.

- Rust 1.79
- Cargo installed
- An Arbitrum chain private key to an account with enough funds

Using:

```
./target/release/activate-stylus-program --help
Usage: activate-stylus-program [OPTIONS] --private-key <PRIVATE_KEY> --endpoint <ENDPOINT> --address <ADDRESS>

Options:
      --private-key <PRIVATE_KEY>            
      --endpoint <ENDPOINT>                  
      --address <ADDRESS>                    
      --bump-fee-percent <BUMP_FEE_PERCENT>  
  -h, --help                                 Print help
  -V, --version                              Print version
```

Example:

```
cargo build --release && \
./target/release/activate-stylus-program --private-key=$PRIV_KEY --address=$ADDR --endpoint=$RPC_URL --bump-fee-percent=20
```


