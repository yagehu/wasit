#/usr/bin/env bash

guix shell \
  --container \
  --network \
  --emulate-fhs \
  --manifest=dev.scm