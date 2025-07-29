#!/usr/bin/env bash

docker build --platform linux/arm64 -t wasit:arm64 .
docker build --platform linux/amd64 -t wasit:amd64 .
docker image save wasit:arm64 | gzip > container-img-arm64.tar.gz
docker image save wasit:amd64 | gzip > container-img-amd64.tar.gz

rm -rf artifact/
mkdir artifact/
cp -r \
  configs executor idxspace runners src \
  compile-time \
  store Cargo.toml Cargo.lock rust-toolchain.toml \
  preview1.witx \
  Dockerfile Containerfile \
  README.md \
  container-img-arm64.tar.gz \
  container-img-amd64.tar.gz \
  artifact/

rm artifact.zip
zip -r artifact.zip artifact

