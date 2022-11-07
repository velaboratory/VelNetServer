FROM rust:1.64 as build

# 1. Create a new empty shell project
RUN USER=root cargo new --bin velnet_server
WORKDIR /velnet_server

# 2. Copy our manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# 3. Build only the dependencies to cache them
RUN cargo build --release && rm src/*.rs

# 4. Now that the dependency is built, copy your source code
COPY ./src ./src

# 5. Build for release.
RUN rm ./target/release/deps/velnet_server*
RUN cargo build --release

# our final base
FROM debian:buster-slim

# copy the build artifact from the build stage
COPY --from=build /velnet_server/target/release/velnet_server .
COPY config.json .

EXPOSE 5000/tcp
EXPOSE 5000/udp

# run
CMD ["./velnet_server"]