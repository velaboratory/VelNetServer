# VelNetServerRust

This basic, single-file relay server is designed to be used for network games, and is similar to Photon Realtime in design.  It is written in Rust, with a single-threaded, non-blocking design and does not rely on any network frameworks (pure TCP/UDP).  A Unity/C# client implementation can be found in our VelNetUnity repository.  

Like Photon, there is no built-in persistence of rooms or data.  Rooms are created when the first client joins and destroyed when the last client leaves.  

The only game logic implemented by the server is that of a "master client", which is an easier way to negotiate a leader in a room that can perform room level operations.

The "group" functionality is used to specify specific clients to communicate with.  Note, these client ids can bridge across rooms.

The server supports both TCP and UDP transports.  

## Running

1. Get a linoox server (also runs fine on windows & osx, but the instructions below are for linux)
2. Clone this repo
3. Edit config.txt to an open port on your firewall
4. Install rust through cargo: e.g. `sudo apt install cargo`
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
