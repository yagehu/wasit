FROM fedora:40 AS base
RUN dnf update -y && dnf install -y wget curl autoconf automake gcc libtool clang perl cmake

FROM base AS rust
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
RUN echo 'source $HOME/.cargo/env' >> $HOME/.bashrc
ENV PATH="/root/.cargo/bin:${PATH}"

FROM base AS wasi-sdk
ARG TARGETARCH
WORKDIR /wasi-sdk
RUN \
    VERSION=25.0 && \
    if [ "$TARGETARCH" = "amd64" ]; then \
        ARCH=x86_64; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        ARCH=arm64; \
    else \
        false || echo "no arch"; \
    fi && \
    wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-25/wasi-sdk-${VERSION}-${ARCH}-linux.tar.gz && \
    tar xvzf wasi-sdk-${VERSION}-${ARCH}-linux.tar.gz --strip-components=1 && \
    rm *.tar.gz

FROM rust AS build-wasit
RUN \
  dnf install -y protobuf-compiler protobuf-devel z3-devel && \
  mkdir -p /protobuf/include && \
  cp -r /usr/include/google /protobuf/include/google
WORKDIR /wasit
ENV WASI_SDK=/wasi-sdk
ENV protobuf_CFLAGS=-I/protobuf/include
COPY --from=wasi-sdk /wasi-sdk $WASI_SDK
COPY compile-time compile-time
COPY executor executor
COPY idxspace idxspace
COPY runners runners
COPY src src
COPY store store
COPY Cargo.toml Cargo.lock rust-toolchain.toml .
RUN cargo build --release

FROM rust AS build-wasmer
COPY runtimes/wasmer /wasmer
WORKDIR /wasmer
ARG TARGETARCH
RUN make build-wasmer

FROM rust AS build-wasmtime
COPY runtimes/wasmtime /wasmtime
WORKDIR /wasmtime
ARG TARGETARCH
RUN \
    if [ "$TARGETARCH" = "amd64" ]; then \
        ARCH=x86_64; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        ARCH=aarch64; \
    else \
        false || echo "no arch"; \
    fi && \
    cargo build --release

FROM base AS build-wamr
COPY runtimes/wasm-micro-runtime /wamr
WORKDIR /wamr
RUN \
  mkdir /build && \
  cmake -DCMAKE_BUILD_TYPE=Release -B /build -S /wamr/product-mini/platforms/linux -DWAMR_BUILD_REF_TYPES=1 && \
  cmake --build /build

FROM base AS build-wasmedge
COPY runtimes/WasmEdge /wasmedge/src
WORKDIR /wasmedge/src
RUN \
  dnf install -y llvm16-devel lld16-devel git && \
  mkdir /wasmedge/build && \
  LLD_DIR=/usr/lib64/lld16 LLVM_DIR=/usr/lib64/llvm16 \
    cmake -DCMAKE_BUILD_TYPE=Release \
    -B /wasmedge/build -S /wasmedge/src && \
  cmake --build /wasmedge/build && \
  cmake --install /wasmedge/build --prefix /wasmedge/install

FROM base AS build-wazero
COPY runtimes/wazero /wazero
WORKDIR /wazero
RUN \
  dnf install -y go && \
  go build cmd/wazero/wazero.go

FROM fedora:40
WORKDIR /wasit
RUN \
  dnf install -y \
  nodejs \
  # WASIT dependency \
  z3-devel \
  # Wasmedge dependency \
  lld16-libs
ENV PATH=/wasit/runtimes:/wasit/runtimes/wasmedge/bin:$PATH
COPY preview1.witx .
COPY configs configs
COPY --from=build-wasit /wasit/target/release/wazzi .
COPY --from=build-wasit /wasit/target/release/wazzi-executor.wasm target/release/wazzi-executor.wasm
COPY --from=build-wasmer /wasmer/target/release/wasmer runtimes/
COPY --from=build-wasmtime /wasmtime/target/release/wasmtime runtimes/
COPY --from=build-wamr /build/iwasm runtimes/
COPY --from=build-wasmedge /wasmedge/install runtimes/wasmedge
COPY --from=build-wazero /wazero/wazero runtimes/
