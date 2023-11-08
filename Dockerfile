FROM rust:bookworm as builder
RUN apt update
RUN apt -y install build-essential
WORKDIR /home/rust/src
COPY . .
RUN cargo build --locked --release
RUN mkdir -p build-out/
RUN cp target/release/routeros-steamcm-iplist build-out/update-steamcm-iplist

FROM busybox:stable-glibc
LABEL authors="gnattu"
WORKDIR /app
COPY --from=builder /lib/aarch64-linux-gnu/libgcc_s.so.1 /lib/aarch64-linux-gnu/libgcc_s.so.1
COPY --from=builder /home/rust/src/build-out/update-steamcm-iplist .
USER 1000:1000
ENTRYPOINT ["./update-steamcm-iplist"]