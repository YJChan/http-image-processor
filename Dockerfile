FROM rust:1.53 as build

RUN USER=root cargo new --bin image-processor
WORKDIR /image-processor

COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

RUN cargo build --release
RUN rm src/*.rs

COPY ./src ./src
COPY ./fonts ./fonts

RUN rm ./target/release/deps/image_processor*
RUN cargo build --release

FROM rust:1.53-slim-buster

COPY --from=build /image-processor/target/release/image-processor .

EXPOSE 8080

CMD ["./image-processor"]