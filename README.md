# VelNetServerRust

This basic, single-file relay server is designed to be used for network games, and is similar to Photon Realtime in design.  It is written in Rust, with a single-threaded, non-blocking design and does not rely on any network frameworks (pure TCP/UDP).  A Unity/C# client implementation can be found in our VelNetUnity repository.  

Like Photon, there is no built-in persistence of rooms or data.  Rooms are created when the first client joins and destroyed when the last client leaves.  

The only game logic implemented by the server is that of a "master client", which is an easier way to negotiate a leader in a room that can perform room level operations.

The "group" functionality is used to specify specific clients to communicate with.  Note, these client ids can bridge across rooms.

The server supports both TCP and UDP transports.  

## Running

1. Get a linoox server (also runs fine on windows & osx, but the instructions below are for linux)
2. Clone this repo
3. Edit config.json to an open port on your firewall
4. Modify the `user` field in `control-panel/config.json` to be your username.
5. Install rust through using rustup: https://rustup.rs/
6. Install: `sudo ./install.sh`
7. Run server: `sudo systemctl start velnet`
8. Run control panel: `sudo systemctl start velnet-control-panel`
9. Install onefetch: `cargo install onefetch`
