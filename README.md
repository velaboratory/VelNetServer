# VelNetServerRust

## Running

1. Get a linoox server
2. Clone this repo
3. Install rust: `curl https://sh.rustup.rs -sSf | sh`
4. Set up env: `source $HOME/.cargo/env` or add to `.bashrc`
5. Build: `cargo build --release`
6. Run: `sudo ./main`
7. Or run in the background so that it doesn't quit when you leave ssh: `nohup sudo ./main`. You'll have to install `nohup` with apt.
