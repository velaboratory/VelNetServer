# VelNet Server

This basic, single-file relay server is designed to be used for network games, and is similar to Photon Realtime in design.  It is written in Rust, with a single-threaded, non-blocking design and does not rely on any network frameworks (pure TCP/UDP).  A Unity/C# client implementation can be found in our [VelNetUnity](https://github.com/velaboratory/VelNetUnity) repository.

Like Photon, there is no built-in persistence of rooms or data.  Rooms are created when the first client joins and destroyed when the last client leaves.  

The only game logic implemented by the server is that of a "master client", which is an easier way to negotiate a leader in a room that can perform room level operations.

The "group" functionality is used to specify specific clients to communicate with.  Note, these client ids can bridge across rooms.

The server supports both TCP and UDP transports.  

## Running

### Option 1: Pull from Docker Hub

```sh
docker run -p 5000:5000 -p 5000:5000/udp velaboratory/velnet
```

or

```sh
docker run -p 5002:5000 -p 5002:5000/udp --name velnet-2 velaboratory/velnet
```

To run on a different port and change the name of the container.

### Option 2: Use docker-compose

Runs both the control panel and the server.

```sh
docker compose up -d
```

to run, and

```sh
docker compose stop
```

to stop.

This builds the images from the local data in the folder, and doesn't pull anything from Docker Hub.

### Option 3: Run Rust natively

1. Clone this repo
2. Edit `config.json` to an open port on your firewall
3. Install Rust: <https://rustup.rs/>
4. `cd server`
5. Debug:
   1. `cargo run`
6. Build:
   1. `cargo build --release`
   2. `./target/release/velnet_server`

### Option 4: Run Rust natively (with control panel and install script)

1. Get a linoox server (also runs fine on Windows & OSX, but the instructions below are for Linux)
2. Clone this repo
3. Edit `config.json` to an open port on your firewall
4. Modify the `user` field in `control-panel/config.json` to be your username.
5. Install rust through using rustup: <https://rustup.rs/>
6. Install: `sudo ./install.sh`
7. Run server: `sudo systemctl start velnet`
8. Run control panel: `sudo systemctl start velnet-control-panel`
9. Install tuptime: `cargo install tuptime`
10. Install onefetch: `cargo install onefetch`

## Developing

### To upload to Docker Hub (internal use only)

If necessary, log into the `velaboratory` account on Docker Hub with `docker login`.

```sh
cd server
docker buildx build --platform linux/amd64,linux/arm64 -t velaboratory/velnet:latest -t velaboratory/velnet:0.3.0 --push .
```

Make sure to change 0.3.0 to the correct version number.

if `buildx` is not set up, use this command to create and switch to the cross-platform builder:

```sh
docker buildx create --name crossbuilder --bootstrap --use
```
