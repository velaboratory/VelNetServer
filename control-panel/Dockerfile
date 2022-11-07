FROM rust:1.64 as build

# 1. Create a new empty shell project
RUN USER=root cargo new --bin velnet_control_panel
WORKDIR /velnet_control_panel

# 2. Copy our manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# 3. Build only the dependencies to cache them
RUN cargo build --release && rm src/*.rs

# 4. Now that the dependency is built, copy your source code
COPY ./src ./src

# 5. Build for release.
RUN rm ./target/release/deps/velnet_control_panel*
RUN cargo build --release

# our final base
FROM rust:1.64-slim
WORKDIR /velnet_control_panel

# RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
RUN apt update && apt install -y tuptime

# copy the build artifact from the build stage
COPY --from=build /velnet_control_panel/target/release/velnet_control_panel .

# Copy the config files and helper scripts
COPY static static
COPY config.json .
COPY onefetch_file.sh .
COPY git_pull.sh .
COPY compile_server.sh .

EXPOSE 8080

# run
CMD ["./velnet_control_panel"]