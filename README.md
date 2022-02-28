# VelNetServerRust

## Running

1. Get a linoox server
2. Clone this repo
3. Install rust: `sudo apt install cargo`
5. Build: `cargo build --release`
6. Run: `sudo ./target/release/VelNetServerRust`
7. Or run in the background so that it doesn't quit when you leave ssh: `nohup sudo ./target/release/VelNetServerRust`. You'll have to install `nohup` with apt.


## Running with control panel server

You don't need to do both of these steps. The control panel runs the other server.

1. Get a linoox server
2. Clone this repo
3. Install rust: `sudo apt install cargo`
3. Switch to control panel: `cd control-panel`
5. Build: `cargo build --release`
6. `touch ../nohup.out ../restarts.log`
6. Run the web server in the background so that it doesn't quit when you leave ssh: `nohup sudo ./target/release/control-panel`. You'll have to install `nohup` with apt.
