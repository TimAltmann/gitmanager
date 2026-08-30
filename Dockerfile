FROM rust:bookworm

# System-Dependencies für eframe/glow + git2 vendored + cross-compile
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libgl1-mesa-dev \
    libxcb1-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev libxkbcommon-dev \
    mingw-w64 \
    clang lld llvm \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Fix for winres: expects `windres` without prefix, but mingw provides `x86_64-w64-mingw32-windres`
RUN ln -sf /usr/bin/x86_64-w64-mingw32-windres /usr/bin/windres

# cargo-xwin für MSVC Cross-Compile (optional, für kleinere/bessere Windows-Binaries)
RUN cargo install cargo-xwin --locked

# Windows Targets für Cross-Compile
RUN rustup target add x86_64-pc-windows-gnu x86_64-pc-windows-msvc

WORKDIR /app

# Cache-Layer: nur Cargo.toml kopieren und fetch
COPY Cargo.toml ./
# Dummy main für fetch
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs && cargo fetch || true
# Echte Sources werden via Volume gemountet, daher hier nicht nötig für dev
# Für `docker build` (ohne Volume) trotzdem alles kopieren
COPY . .

# Default: cargo build
CMD ["cargo", "build"]
