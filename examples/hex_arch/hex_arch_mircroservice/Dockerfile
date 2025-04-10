
FROM rust:1.70 AS builder
WORKDIR /usr/src/hex_arch_mircroservice
COPY . .
RUN cargo build --release

FROM debian:buster-slim
COPY --from=builder /usr/src/hex_arch_mircroservice/target/release/hex_arch_mircroservice /usr/local/bin/app
CMD ["app"]
