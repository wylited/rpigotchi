## Compiling
Add the arm target `rustup target add arm-unknown-linux-gnueabihf`
and ensure you have a arm compiler like `gcc-arm-none-eabi-bin` for cross compilation

compile with simple `cargo build --release`, hopefully it works.

then just transfer it to the rpi02w with `scp target/arm-unknown-linux-gnueabihf/release`

make sure to run with superuser privelleges!
