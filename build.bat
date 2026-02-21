@echo off
set RUSTUP_HOME=D:\rust\rustup
set CARGO_HOME=D:\rust\cargo
set TEMP=D:\temp
set TMP=D:\temp
set PATH=D:\rust\cargo\bin;%PATH%
call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
cargo build %*
