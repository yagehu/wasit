Example building and running on Windows:

powershell -Command { $env:WASI_SDK="C:\Users\yagej\src\diva\wasi-sdk-25.0-x86_64-windows"; $env:Protobuf_DIR="C:\Users\yagej\.local\protobuf\3.19"; $env:Z3_SYS_Z3_HEADER="C:\Users\yagej\scoop\apps\z3\current\include\z3.h"; $env:LIB="C:\Users\yagej\.local\z3\4.14.1\lib"; $env:Path+="C:\Users\yagej\.local\z3\4.14.1\bin"; $env:Path+=";C:\Users\yagej\src\diva\diva\runtimes\wasm-micro-runtime\product-mini\platforms\windows\build\Release"; $env:Path+=";C:\Users\yagej\src\diva\diva\runtimes\wasmtime\target\release"; cargo run --release -- -c 1 --strategy stateful --time-limit 5s .\configs\wamr-wasmtime.yaml .\abc }

Compile WAMR on Windows

```
cmake .. -DWAMR_BUILD_REF_TYPES=1
cmake --build . --config release
```
